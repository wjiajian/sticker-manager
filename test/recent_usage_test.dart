import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:sticker_manager/services/recent_usage.dart';
import 'package:sticker_manager/services/preferences.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUp(() => SharedPreferences.setMockInitialValues({}));
  test('history persists, deduplicates, caps at 20, and prunes deleted IDs',
      () async {
    final store = RecentUsage();
    for (var i = 0; i < 25; i++) {
      await store.record('$i');
    }
    await store.record('10');
    final ids = await RecentUsage().ids();
    expect(ids.length, 20);
    expect(ids.first, '10');
    expect(ids.toSet().length, 20);
    expect(ids, isNot(contains('0')));
    await store.retain({'10', '24'});
    expect(await store.ids(), ['10', '24']);
  });
  test('simultaneous successful actions retain every history entry', () async {
    final store = RecentUsage();
    await Future.wait(
        [store.record('a'), store.record('b'), store.record('a')]);
    expect(await store.ids(), ['a', 'b']);
  });
  test('recent visibility defaults on and persists independently', () async {
    final prefs = AppPreferences();
    expect(await prefs.showRecent(), isTrue);
    await prefs.setShowRecent(false);
    expect(await AppPreferences().showRecent(), isFalse);
  });
}
