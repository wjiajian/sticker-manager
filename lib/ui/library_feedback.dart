import 'package:flutter/material.dart';

import 'app_theme.dart';

/// Bottom-centered feedback for the content area. Operation results show a
/// small transient pill; import progress is a separate pill stacked below it
/// so the two never overlap.
class LibraryFeedback extends StatelessWidget {
  const LibraryFeedback({
    super.key,
    this.message,
    required this.progressActive,
    required this.progressText,
  });

  /// Transient operation result, e.g. 已复制 / 已发送 / a failure reason.
  final String? message;

  /// Whether a longer-running task (import, delete) is in progress.
  final bool progressActive;
  final String progressText;

  @override
  Widget build(BuildContext context) {
    final showProgress = progressActive || progressText.isNotEmpty;
    if (message == null && !showProgress) return const SizedBox.shrink();
    return Positioned(
      left: 16,
      right: 16,
      bottom: 40,
      child: IgnorePointer(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (message != null)
              _Pill(
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(Icons.check_circle,
                        size: 28, color: AppTheme.accent),
                    const SizedBox(width: 12),
                    Flexible(
                      child: Text(
                        message!,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(fontSize: 18),
                      ),
                    ),
                  ],
                ),
              ),
            if (message != null && showProgress) const SizedBox(height: 8),
            if (showProgress)
              _Pill(
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (progressActive) ...[
                      const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      ),
                      const SizedBox(width: 8),
                    ],
                    Flexible(
                      child: Text(
                        progressText,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(fontSize: 13),
                      ),
                    ),
                  ],
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _Pill extends StatelessWidget {
  const _Pill({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 520),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 16),
        decoration: BoxDecoration(
          color: AppTheme.cardBackground,
          borderRadius: BorderRadius.circular(16),
          border: Border.all(color: AppTheme.border),
          boxShadow: const [
            BoxShadow(
              color: Color(0x140F2438),
              blurRadius: 24,
              offset: Offset(0, 6),
            ),
          ],
        ),
        child: child,
      ),
    );
  }
}
