"""Bake existing tracker meshes into small native mesh assets without a web renderer."""
import json,struct,base64
from pathlib import Path
root=Path(__file__).resolve().parents[2]
for name in ['tracker','extension']:
 doc=json.loads((root/f'gui/public/models/{name}.gltf').read_text());buffers=[base64.b64decode(b['uri'].split(',')[1]) for b in doc['buffers']];triangles=[]
 def read(index):
  a=doc['accessors'][index];v=doc['bufferViews'][a['bufferView']];fmt={5126:'f',5125:'I',5123:'H',5121:'B'}[a['componentType']];count={'SCALAR':1,'VEC3':3,'VEC2':2,'VEC4':4}[a['type']];stride=v.get('byteStride',struct.calcsize(fmt)*count);start=v.get('byteOffset',0)+a.get('byteOffset',0);return [struct.unpack_from('<'+fmt*count,buffers[v['buffer']],start+i*stride) for i in range(a['count'])]
 for mesh in doc['meshes']:
  for primitive in mesh['primitives']:
   positions=read(primitive['attributes']['POSITION']);indices=[i[0] for i in read(primitive['indices'])];material=doc.get('materials',[{}])[primitive.get('material',0)];color=material.get('pbrMetallicRoughness',{}).get('baseColorFactor',[.6,.4,.7,1])[:3]
   for i in range(0,len(indices),3):triangles.append({'points':[positions[j] for j in indices[i:i+3]],'color':color})
 (root/f'gui-gpui/src/{name}_mesh.json').write_text(json.dumps(triangles,separators=(',',':'))+'\n');print(name,len(triangles),'triangles')
