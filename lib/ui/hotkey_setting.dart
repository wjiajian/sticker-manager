import 'package:flutter/material.dart';
import 'package:hotkey_manager/hotkey_manager.dart';

/// Records a draft shortcut inline; registration happens only on save.
class HotkeySetting extends StatefulWidget {
  const HotkeySetting({super.key, required this.current, required this.onSave});
  final HotKey current;
  final Future<bool> Function(HotKey) onSave;

  @override
  State<HotkeySetting> createState() => _HotkeySettingState();
}

class _HotkeySettingState extends State<HotkeySetting> {
  bool _editing = false;
  bool _saving = false;
  HotKey? _draft;
  String? _message;

  Future<void> _save() async {
    final draft = _draft;
    if (draft == null || _saving) return;
    if ((draft.modifiers ?? []).isEmpty ||
        HotKeyModifier.values.any(
            (modifier) => modifier.physicalKeys.contains(draft.physicalKey))) {
      setState(() => _message = '请使用包含修饰键和普通按键的组合键');
      return;
    }
    setState(() {
      _saving = true;
      _message = null;
    });
    bool saved;
    try {
      saved = await widget.onSave(draft);
    } catch (_) {
      saved = false;
    }
    if (!mounted) return;
    setState(() {
      _saving = false;
      _editing = !saved;
      _message = saved ? '快捷键已保存' : '无法保存，快捷键可能已被占用，请换一组重试';
    });
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 8, 0, 8),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Row(children: [
          const Expanded(child: Text('快速唤出快捷键')),
          const SizedBox(width: 12),
          Expanded(
            child: Align(
              alignment: Alignment.centerRight,
              child: _editing
                  ? Container(
                      key: const ValueKey('hotkey-editor'),
                      padding: const EdgeInsets.all(8),
                      decoration: BoxDecoration(
                          border: Border.all(
                              color: Theme.of(context).colorScheme.primary),
                          borderRadius: BorderRadius.circular(6)),
                      child: _saving
                          ? const Text('正在保存…')
                          : HotKeyRecorder(
                              initalHotKey: widget.current,
                              onHotKeyRecorded: (value) => setState(() {
                                _draft = value;
                                _message = null;
                              }),
                            ),
                    )
                  : OutlinedButton(
                      onPressed: () => setState(() {
                        _editing = true;
                        _draft = null;
                        _message = null;
                      }),
                      child: Row(mainAxisSize: MainAxisSize.min, children: [
                        Flexible(
                            child: HotKeyVirtualView(hotKey: widget.current)),
                        const SizedBox(width: 8),
                        const Icon(Icons.edit_outlined, size: 16),
                      ]),
                    ),
            ),
          ),
        ]),
        if (_editing) ...[
          const SizedBox(height: 8),
          const Text('按下新的组合键，至少包含一个修饰键。'),
          Row(mainAxisAlignment: MainAxisAlignment.end, children: [
            TextButton(
                onPressed: _saving
                    ? null
                    : () => setState(() {
                          _editing = false;
                          _message = null;
                        }),
                child: const Text('取消')),
            TextButton(
                onPressed: _saving || _draft == null ? null : _save,
                child: const Text('保存')),
          ]),
        ],
        if (_message != null)
          Text(_message!, style: Theme.of(context).textTheme.bodySmall),
      ]),
    );
  }
}
