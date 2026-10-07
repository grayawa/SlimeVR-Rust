#!/usr/bin/env python3
"""Extract the original React SVG icons for the native frontend, without redrawing."""
from pathlib import Path
import re
import xml.etree.ElementTree as ET
ROOT = Path(__file__).resolve().parents[2]
NAMES = ['Home','Human','Ski','Ruler','Wifi','Gear','Chest','Hip','UpperLeg','Ankle','Foot','Headset','Controller','UpperArm','LowerArm','Shoulder','Neck','Waist','UpperChest','SlimeVR','Steam','Wrench','Router','VRC','VMC','Squares','Check','Bell','Bug','ArrowRightLeft','Bulb','USB','Sitting']
ATTRIBUTES = {'fillRule':'fill-rule','clipRule':'clip-rule','strokeMiterlimit':'stroke-miterlimit','strokeWidth':'stroke-width','strokeLinecap':'stroke-linecap','strokeLinejoin':'stroke-linejoin'}
for name in NAMES:
    source_name = 'UsbIcon' if name == 'USB' else 'ArrowIcons' if name == 'ArrowRightLeft' else f'{name}Icon{"s" if name == "Wrench" else ""}'
    source = (ROOT / f'gui/src/components/commons/icon/{source_name}.tsx').read_text()
    svg = re.findall(r'<svg\b[\s\S]*?</svg>', source)[-1]
    # The disabled headset cross is an optional overlay, not the base icon.
    svg = re.sub(r'\{disabled\s*&&\s*\([\s\S]*?\)\}', '', svg)
    svg = re.sub(r'\s(?:width|height|className|style|transform)=\{[\s\S]*?\}(?=\s|>)', '', svg)
    svg = re.sub(r'\sfill=\{[^}]+\}', '', svg)
    for jsx, xml in ATTRIBUTES.items():
        svg = svg.replace(jsx, xml)
    svg = re.sub(r'=\{([0-9.]+)\}', r'="\1"', svg)
    if name == 'Bell': svg = svg.replace('<svg', '<svg stroke="black" fill="none"', 1)
    svg = svg.replace('stroke="currentColor"', 'stroke="black"')
    ET.fromstring(svg)
    (ROOT / f'gui-gpui/assets/icons/{name}.svg').write_text(svg+'\n')
print(f'Extracted {len(NAMES)} original icons')
