from pathlib import Path
root=Path(__file__).resolve().parents[2]
out=['// Generated: retain every original SlimeVR language.','pub const LOCALES: &[&str] = &['];locales=[]
for p in sorted((root/'gui/public/i18n').glob('*/translation.ftl')):
 locales.append(p.parent.name);out.append('"'+p.parent.name+'",')
out+= ['];','pub fn source(locale: &str) -> Option<&\'static str> {match locale {']
for locale in locales:out.append(f'"{locale}" => Some(include_str!("../../gui/public/i18n/{locale}/translation.ftl")),')
out+=['_=>None,}}']
import re
pairs=re.findall(r"name: '([^']+)'\s*,\s*key: '([^']+)'",(root/'gui/src/i18n/names.ts').read_text())
out+=['/// Original language picker display names.','pub fn name(locale: &str) -> &str {match locale {']
for name,key in pairs:out.append(f'"{key}" => "{name}",')
out+=['_=>locale,}}']
(root/'gui-gpui/src/locales.rs').write_text('\n'.join(out)+'\n')
print('Embedded',len(locales),'languages')
