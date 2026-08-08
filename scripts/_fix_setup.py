import io
path = r'src/windows/SetupWindow.tsx'
with open(path, 'r', encoding='utf-8') as f:
    t = f.read()
# 找损坏点：</label>：{([密钥]) === 实际是 settings.emergency_hotkey
# 用特征: </label> 后紧跟全角冒号再 {
needle = '</label>\uff1a{('
idx = t.find(needle)
print('idx=', idx)
if idx >= 0:
    # 重建：保留 </label>，插入 <p> 行与紧急快捷键文字前缀
    replacement = '</label>\n              <p className="text-sm text-slate-400">\n                \u7d27\u6025\u5feb\u6377\u952e\uff1a{('
    t = t[:idx] + replacement + t[idx+len(needle):]
    with open(path, 'w', encoding='utf-8') as f:
        f.write(t)
    print('fixed')
