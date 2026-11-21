// 只给悬浮球 WKWebView 增加 AppKit 拖拽格式；普通文件仍交给 Wry。
#import <AppKit/AppKit.h>
#import <dispatch/dispatch.h>
#import <objc/runtime.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

extern void sticker_macos_drop_free_app(void *app);
extern void sticker_macos_drop_active(void *app, bool active);
extern void *sticker_macos_drop_begin(void *app);
extern const char *sticker_macos_drop_directory(void *batch);
extern void sticker_macos_drop_error(void *batch, const char *message);
extern void sticker_macos_drop_enqueue(void *batch, const char *const *paths, size_t count);
extern void sticker_macos_drop_free_batch(void *batch);

static const NSUInteger kMaxFiles = 256;
static const NSUInteger kMaxBytes = 100 * 1024 * 1024;
static char kStateKey;

@interface StickerDropState : NSObject {
@public
    void *app;
    BOOL customDrag;
}
@end
@implementation StickerDropState
- (void)dealloc {
    if (app) sticker_macos_drop_free_app(app);
}
@end

// block 保留这个对象；异步接收和排队导入期间 Rust 的临时目录保持存活。
@interface StickerDropBatch : NSObject {
@public
    void *batch;
}
@end
@implementation StickerDropBatch
- (void)dealloc {
    if (batch) sticker_macos_drop_free_batch(batch);
}
@end

static NSArray<NSPasteboardType> *imageTypes(void) {
    // 优先保留动画，最后才选只能表示静态图片的 TIFF。
    return @[@"com.compuserve.gif", @"org.webmproject.webp", @"public.webp",
             NSPasteboardTypePNG, @"public.jpeg", NSPasteboardTypeTIFF];
}

static NSOperationQueue *promiseQueue(void) {
    static NSOperationQueue *queue;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        queue = [[NSOperationQueue alloc] init];
        queue.name = @"sticker-file-promises";
        queue.maxConcurrentOperationCount = 2;
    });
    return queue;
}

static void batchError(StickerDropBatch *batch, NSString *message) {
    sticker_macos_drop_error(batch->batch, message.UTF8String);
}

static BOOL hasFileURL(NSPasteboard *board) {
    NSArray *urls = [board readObjectsForClasses:@[[NSURL class]]
                                         options:@{ NSPasteboardURLReadingFileURLsOnlyKey: @YES }];
    for (id value in urls) {
        if (![value isKindOfClass:[NSURL class]] || ![(NSURL *)value isFileURL]) continue;
        NSString *path = [(NSURL *)value filePathURL].path;
        // 只检查本地路径的可读性，不请求下载；保留目录与压缩包的原有导入路径。
        if (path.length > 0 && [[NSFileManager defaultManager] isReadableFileAtPath:path]) return YES;
    }
    return NO;
}

static BOOL hasCustomData(NSPasteboard *board) {
    return [board availableTypeFromArray:[NSFilePromiseReceiver readableDraggedTypes]] != nil ||
           [board availableTypeFromArray:imageTypes()] != nil;
}

static void enqueueFiles(StickerDropBatch *batch, NSArray<NSURL *> *files) {
    if (files.count == 0 || files.count > kMaxFiles) {
        batchError(batch, @"拖入文件为空或超过 256 项");
        return;
    }
    const char **paths = calloc(files.count, sizeof(*paths));
    if (!paths) {
        batchError(batch, @"无法分配拖入文件列表");
        return;
    }
    for (NSUInteger i = 0; i < files.count; i++) paths[i] = files[i].fileSystemRepresentation;
    sticker_macos_drop_enqueue(batch->batch, paths, files.count);
    free(paths);
}

// 不依赖扩展名或剪贴板类型标签：某些来源用 TIFF 类型发布 PNG/GIF 数据。
static NSString *knownExtension(NSData *data) {
    const unsigned char *bytes = data.bytes;
    NSUInteger length = data.length;
    if (length >= 8 && memcmp(bytes, "\x89PNG\r\n\x1a\n", 8) == 0) return @"png";
    if (length >= 6 && (memcmp(bytes, "GIF87a", 6) == 0 || memcmp(bytes, "GIF89a", 6) == 0)) return @"gif";
    if (length >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff) return @"jpg";
    if (length >= 12 && memcmp(bytes, "RIFF", 4) == 0 && memcmp(bytes + 8, "WEBP", 4) == 0) return @"webp";
    return nil;
}

static BOOL receiveImages(NSPasteboard *board, StickerDropBatch *batch, NSURL *directory) {
    NSMutableArray<NSDictionary *> *images = [NSMutableArray array];
    NSUInteger total = 0;
    for (NSPasteboardItem *item in board.pasteboardItems ?: @[]) {
        NSPasteboardType type = [item availableTypeFromArray:imageTypes()];
        if (!type) continue;
        NSData *data = [item dataForType:type];
        if (!data || data.length == 0) {
            batchError(batch, @"拖入来源没有提供图片数据");
            return NO;
        }
        if (images.count >= kMaxFiles || data.length > kMaxBytes - total) {
            batchError(batch, @"拖入图片超过 256 项或总量超过 100 MB");
            return NO;
        }
        total += data.length;
        [images addObject:@{ @"data": [data copy], @"type": type }];
    }
    if (images.count == 0) {
        batchError(batch, @"拖入来源没有提供可读取的图片");
        return NO;
    }
    // 数据必须在拖放回调中取得；编码和文件写入不阻塞主线程。
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        @autoreleasepool {
            NSMutableArray<NSURL *> *files = [NSMutableArray array];
            for (NSDictionary *entry in images) {
                NSData *data = entry[@"data"];
                NSString *type = entry[@"type"];
                NSString *extension = knownExtension(data);
                if (!extension && [type isEqualToString:NSPasteboardTypeTIFF]) {
                    NSBitmapImageRep *bitmap = [NSBitmapImageRep imageRepWithData:data];
                    data = [bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
                    extension = @"png";
                }
                if (!data || !extension) {
                    batchError(batch, @"无法识别拖入的图片格式");
                    continue;
                }
                NSURL *file = [directory URLByAppendingPathComponent:
                    [NSString stringWithFormat:@"拖入表情-%lu.%@", (unsigned long)files.count + 1, extension]];
                NSError *error = nil;
                if (![data writeToURL:file options:NSDataWritingAtomic error:&error]) {
                    batchError(batch, error.localizedDescription ?: @"无法保存拖入图片");
                    continue;
                }
                [files addObject:file];
            }
            if (files.count) enqueueFiles(batch, files);
        }
    });
    return YES;
}

static BOOL receivePromises(NSPasteboard *board, StickerDropBatch *batch, NSURL *directory) {
    NSArray<NSFilePromiseReceiver *> *receivers =
        [board readObjectsForClasses:@[[NSFilePromiseReceiver class]] options:nil];
    if (receivers.count == 0) return NO;
    if (receivers.count > kMaxFiles) {
        batchError(batch, @"拖入文件超过 256 项");
        return YES;
    }
    for (NSFilePromiseReceiver *receiver in receivers) {
        // 系统要求一次拖拽的所有文件承诺使用同一个目的目录。
        [receiver receivePromisedFilesAtDestination:directory options:@{} operationQueue:promiseQueue()
                                             reader:^(NSURL *fileURL, NSError *error) {
            if (error || !fileURL.isFileURL) {
                batchError(batch, error.localizedDescription ?: @"无法接收拖入文件");
            } else {
                enqueueFiles(batch, @[fileURL]);
            }
        }];
    }
    return YES;
}

static IMP originalEnter;
static IMP originalUpdate;
static IMP originalPrepare;
static IMP originalPerform;
static IMP originalExit;
static IMP originalConclude;

static NSDragOperation dropEnter(id view, SEL selector, id<NSDraggingInfo> info) {
    StickerDropState *state = objc_getAssociatedObject(view, &kStateKey);
    if (state->customDrag) sticker_macos_drop_active(state->app, false);
    state->customDrag = NO;
    @try {
        NSPasteboard *board = info.draggingPasteboard;
        if (!hasFileURL(board) && hasCustomData(board) &&
            (info.draggingSourceOperationMask & NSDragOperationCopy)) {
            state->customDrag = YES;
            sticker_macos_drop_active(state->app, true);
            return NSDragOperationCopy;
        }
    } @catch (NSException *exception) {
        NSLog(@"无法读取拖拽类型：%@", exception.reason);
    }
    return ((NSDragOperation (*)(id, SEL, id<NSDraggingInfo>))originalEnter)(view, selector, info);
}

static NSDragOperation dropUpdate(id view, SEL selector, id<NSDraggingInfo> info) {
    StickerDropState *state = objc_getAssociatedObject(view, &kStateKey);
    if (state->customDrag) return NSDragOperationCopy;
    return ((NSDragOperation (*)(id, SEL, id<NSDraggingInfo>))originalUpdate)(view, selector, info);
}

static BOOL dropPrepare(id view, SEL selector, id<NSDraggingInfo> info) {
    StickerDropState *state = objc_getAssociatedObject(view, &kStateKey);
    if (state->customDrag) return YES;
    return ((BOOL (*)(id, SEL, id<NSDraggingInfo>))originalPrepare)(view, selector, info);
}

static BOOL dropPerform(id view, SEL selector, id<NSDraggingInfo> info) {
    StickerDropState *state = objc_getAssociatedObject(view, &kStateKey);
    if (!state->customDrag) return ((BOOL (*)(id, SEL, id<NSDraggingInfo>))originalPerform)(view, selector, info);
    // 本次 session 的归属保留到 conclude/下一次 enter；失败后的 exit 也不能转发给 Wry。
    sticker_macos_drop_active(state->app, false);
    StickerDropBatch *batch = [[StickerDropBatch alloc] init];
    batch->batch = sticker_macos_drop_begin(state->app);
    if (!batch->batch) return NO;
    const char *path = sticker_macos_drop_directory(batch->batch);
    if (!path) {
        batchError(batch, @"无法创建拖入临时目录");
        return NO;
    }
    NSURL *directory = [NSURL fileURLWithFileSystemRepresentation:path isDirectory:YES relativeToURL:nil];
    @try {
        NSPasteboard *board = info.draggingPasteboard;
        if (receivePromises(board, batch, directory)) return YES;
        return receiveImages(board, batch, directory);
    } @catch (NSException *exception) {
        batchError(batch, exception.reason ?: @"无法读取拖入数据");
        return NO;
    }
}

static void dropExit(id view, SEL selector, id<NSDraggingInfo> info) {
    StickerDropState *state = objc_getAssociatedObject(view, &kStateKey);
    if (state->customDrag) {
        sticker_macos_drop_active(state->app, false);
    } else {
        ((void (*)(id, SEL, id<NSDraggingInfo>))originalExit)(view, selector, info);
    }
}

static void dropConclude(id view, SEL selector, id<NSDraggingInfo> info) {
    StickerDropState *state = objc_getAssociatedObject(view, &kStateKey);
    if (state->customDrag) {
        state->customDrag = NO;
    } else {
        ((void (*)(id, SEL, id<NSDraggingInfo>))originalConclude)(view, selector, info);
    }
}

static BOOL addOverride(Class subclass, Class parent, SEL selector, IMP replacement, IMP *original) {
    Method method = class_getInstanceMethod(parent, selector);
    if (!method) return NO;
    *original = method_getImplementation(method);
    return class_addMethod(subclass, selector, replacement, method_getTypeEncoding(method));
}

bool sticker_macos_install(void *rawView, void *app) {
    NSView *view = (__bridge NSView *)rawView;
    if (!view || !app) return false;
    Class parent = object_getClass(view);
    static Class subclass;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        Class candidate = objc_allocateClassPair(parent, "StickerManagerDropWebView", 0);
        if (!candidate) return;
        BOOL ok = addOverride(candidate, parent, @selector(draggingEntered:), (IMP)dropEnter, &originalEnter) &&
                  addOverride(candidate, parent, @selector(draggingUpdated:), (IMP)dropUpdate, &originalUpdate) &&
                  addOverride(candidate, parent, @selector(prepareForDragOperation:), (IMP)dropPrepare, &originalPrepare) &&
                  addOverride(candidate, parent, @selector(performDragOperation:), (IMP)dropPerform, &originalPerform) &&
                  addOverride(candidate, parent, @selector(draggingExited:), (IMP)dropExit, &originalExit) &&
                  addOverride(candidate, parent, @selector(concludeDragOperation:), (IMP)dropConclude, &originalConclude);
        if (ok) {
            objc_registerClassPair(candidate);
            subclass = candidate;
        } else {
            objc_disposeClassPair(candidate);
        }
    });
    if (!subclass || class_getSuperclass(subclass) != parent) return false;
    StickerDropState *state = [[StickerDropState alloc] init];
    objc_setAssociatedObject(view, &kStateKey, state, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    object_setClass(view, subclass);
    @try {
        NSArray *types = [view.registeredDraggedTypes arrayByAddingObjectsFromArray:imageTypes()];
        [view registerForDraggedTypes:[types arrayByAddingObjectsFromArray:[NSFilePromiseReceiver readableDraggedTypes]]];
    } @catch (NSException *exception) {
        object_setClass(view, parent);
        objc_setAssociatedObject(view, &kStateKey, nil, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        NSLog(@"无法注册图片拖拽：%@", exception.reason);
        return false;
    }
    state->app = app;
    return true;
}
