"""Reuse original form Fluent labels for native settings controls."""
import re,json
from pathlib import Path
root=Path(__file__).resolve().parents[2]
rows=[]
for p in (root/'gui/src/components').rglob('*.tsx'):
 s=p.read_text()
 for m in re.finditer(r'<(?:CheckBox|Input|NumberSelector|Slider|Dropdown|Radio)\b[\s\S]*?/>',s):
  block=m.group();n=re.search(r'name="([\w.]+)"',block)
  if not n:continue
  l=re.search(r'label=\{(?:[\s\S]*?)getString\(\s*[\'\"]([\w-]+)',block)
  if not l:
   prefix=s[max(0,m.start()-500):m.start()];lm=list(re.finditer(r'<Localized\s+id="([\w-]+)"',prefix));l=lm[-1] if lm else None
  if l:rows.append({'path':n.group(1),'label':l.group(1)})
(root/'gui-gpui/src/settings_labels.json').write_text(json.dumps(rows,ensure_ascii=False,indent=2)+'\n')
print('Reused',len(rows),'original labels')
