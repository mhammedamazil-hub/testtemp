"""Genuine bpy scene producer. blender -b --python studio/blender_scene.py -- spec.json out/
Also supports python (official bpy wheel). No fabricated render fallback.
"""
import argparse, hashlib, json, math, sys
from pathlib import Path

def generate(spec_path,out):
 try:import bpy
 except (ImportError,OSError) as exc:
  raise RuntimeError('Blender unavailable: '+str(exc)+'. Install Blender plus Linux runtime libraries, or use the production CI job. No 3D asset was generated.') from exc
 from mathutils import Vector
 spec=json.loads(Path(spec_path).read_text());out=Path(out);out.mkdir(parents=True,exist_ok=True)
 recipe=spec.get('recipe','product');resolution=int(spec.get('resolution',1200));samples=int(spec.get('samples',32))
 if recipe not in ['product','architecture','molecule','membrane','abstract','typography','import']:raise ValueError('Unsupported scene recipe')
 if not 256<=resolution<=4096 or not 8<=samples<=256:raise ValueError('Resolution/samples out of bounds')
 key=hashlib.sha256(Path(__file__).read_bytes()+Path(spec_path).read_bytes()+bpy.app.version_string.encode()).hexdigest()
 if (out/'manifest.json').exists():
  m=json.loads((out/'manifest.json').read_text())
  if m.get('cache_key')==key and all((out/f).exists() for f in m['files']):return m
 bpy.ops.wm.read_factory_settings(use_empty=True);scene=bpy.context.scene
 scene.render.engine='CYCLES';scene.cycles.device='CPU';scene.cycles.samples=samples;scene.cycles.use_denoising=True;scene.cycles.seed=17
 devices=['CPU']
 # CPU is deliberately the portable default. Explicit GPU selection is opportunistic.
 if spec.get('device')=='auto':
  prefs=bpy.context.preferences.addons['cycles'].preferences
  for backend in ['OPTIX','CUDA','HIP','ONEAPI','METAL']:
   try:
    prefs.compute_device_type=backend;prefs.get_devices()
    gpu=[d for d in prefs.devices if d.type!='CPU']
    if gpu:
     for d in prefs.devices:d.use=d in gpu
     scene.cycles.device='GPU';devices=[d.name for d in gpu];break
   except (TypeError,RuntimeError):continue
 scene.render.resolution_x=scene.render.resolution_y=resolution;scene.render.resolution_percentage=100
 scene.render.film_transparent=True;scene.render.image_settings.file_format='PNG';scene.render.image_settings.color_mode='RGBA'
 scene.world=bpy.data.worlds.new('Studio world');scene.world.use_nodes=True;scene.world.node_tree.nodes['Background'].inputs['Color'].default_value=(.12,.15,.19,1)
 def mat(name,color,metal=0,rough=.35):
  m=bpy.data.materials.new(name);m.use_nodes=True;n=m.node_tree.nodes.get('Principled BSDF');n.inputs['Base Color'].default_value=(*color,1);n.inputs['Metallic'].default_value=metal;n.inputs['Roughness'].default_value=rough;return m
 ceramic=mat('Ceramic',(.63,.72,.66),.2);dark=mat('Graphite',(.035,.055,.06),.65);warm=mat('Copper',(.55,.22,.08),.8)
 def cube(name,loc,scale,material):
  bpy.ops.mesh.primitive_cube_add(size=1,location=loc);o=bpy.context.object;o.name=name;o.scale=scale;bpy.ops.object.transform_apply(location=False,rotation=False,scale=True);o.data.materials.append(material)
  mod=o.modifiers.new('Machined edge','BEVEL');mod.width=.04;mod.segments=3;o.modifiers.new('Normals','WEIGHTED_NORMAL');return o
 if recipe=='product':
  cube('Enclosure',(0,0,0),(2.2,.75,2.7),dark);cube('Front panel',(0,-.4,0),(1.95,.06,2.35),ceramic)
  for x in [-.65,0,.65]:cube('Vent',(x,-.45,.55),(.18,.05,.7),dark)
 elif recipe=='architecture':
  cube('Plinth',(0,0,-.2),(4,3,.25),ceramic)
  for x in [-1.3,1.3]:cube('Load bearing wall',(x,0,.8),(.15,2.4,1.8),ceramic)
  cube('Roof',(0,0,1.85),(3.4,2.8,.16),warm);cube('Rear wall',(0,1.15,.7),(2.6,.12,1.6),ceramic)
 elif recipe=='membrane':
  # Stylized phospholipid bilayer: molecular schematic, not atomistic simulation.
  for x in [-1.8,-1.2,-.6,0,.6,1.2,1.8]:
   for y in [-.6,0,.6]:
    for sign in [-1,1]:
     bpy.ops.mesh.primitive_uv_sphere_add(segments=24,ring_count=12,radius=.22,location=(x,y,sign*.8));bpy.context.object.data.materials.append(ceramic);bpy.ops.object.shade_smooth()
     for dx in [-.08,.08]:
      bpy.ops.mesh.primitive_cylinder_add(vertices=12,radius=.045,depth=.55,location=(x+dx,y,sign*.35));bpy.context.object.data.materials.append(warm)
 elif recipe=='molecule':
  # Schematic water molecule; geometry angle 104.5 degrees, not an electron-density model.
  for loc,r,material in [((0,0,0),.7,warm),((1.4*math.sin(math.radians(52.25)),0,1.4*math.cos(math.radians(52.25))),.4,ceramic),((-1.4*math.sin(math.radians(52.25)),0,1.4*math.cos(math.radians(52.25))),.4,ceramic)]:
   bpy.ops.mesh.primitive_uv_sphere_add(segments=48,ring_count=24,radius=r,location=loc);bpy.context.object.data.materials.append(material);bpy.ops.object.shade_smooth()
  for x in [-1.1,1.1]:
   end=Vector((x,0,.85));bpy.ops.mesh.primitive_cylinder_add(vertices=32,radius=.12,depth=end.length,location=end/2);o=bpy.context.object;o.rotation_euler=end.to_track_quat('Z','Y').to_euler();o.data.materials.append(dark)
 elif recipe=='abstract':
  for i in range(5):o=cube('Stack',(0,0,i*.45-1),(2-i*.25,2-i*.25,.22),ceramic);o.rotation_euler.z=i*.25
 elif recipe=='typography':
  bpy.ops.object.text_add();o=bpy.context.object;o.data.body=spec.get('text','FORM');o.data.extrude=.12;o.data.bevel_depth=.015;o.data.align_x='CENTER';o.data.materials.append(warm)
 elif recipe=='import':
  model=Path(spec['model']).resolve()
  if not spec.get('license') or not model.exists():raise ValueError('Import requires existing model and license')
  if model.suffix.lower() in ['.glb','.gltf']:bpy.ops.import_scene.gltf(filepath=str(model))
  elif model.suffix.lower()=='.obj':bpy.ops.wm.obj_import(filepath=str(model))
  else:raise ValueError('Supported model formats: glTF/GLB/OBJ')
 for name,loc,power,size in [('Key',(3,-4,6),900,5),('Fill',(-4,-2,2),500,4),('Rim',(2,4,5),1100,3)]:
  bpy.ops.object.light_add(type='AREA',location=loc);o=bpy.context.object;o.name=name;o.data.energy=power;o.data.shape='DISK';o.data.size=size;o.rotation_euler=(-o.location).to_track_quat('-Z','Y').to_euler()
 bpy.ops.object.camera_add();cam=bpy.context.object;scene.camera=cam;cam.data.type='ORTHO';cam.data.ortho_scale=6
 files=['scene.blend'];views=spec.get('angles',[35,65])
 for i,angle in enumerate(views):
  a=math.radians(angle);cam.location=(6*math.sin(a),-6*math.cos(a),3.5);cam.rotation_euler=(Vector((0,0,.35))-cam.location).to_track_quat('-Z','Y').to_euler()
  filename=f'view-{i+1:02}.png';scene.render.filepath=str((out/filename).resolve());bpy.ops.render.render(write_still=True);files.append(filename)
 bpy.ops.wm.save_as_mainfile(filepath=str((out/'scene.blend').resolve()))
 m=dict(cache_key=key,blender=bpy.app.version_string,devices=devices,recipe=recipe,files=files,source=str(spec_path),license=spec.get('license','Original procedural geometry; project-authored'))
 (out/'manifest.json').write_text(json.dumps(m,indent=2));return m

def main():
 p=argparse.ArgumentParser();p.add_argument('spec');p.add_argument('output');a=p.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else None)
 try:print(json.dumps(generate(a.spec,a.output),indent=2))
 except (RuntimeError,ValueError,OSError) as e:print(str(e),file=sys.stderr);return 1
 return 0
if __name__=='__main__':sys.exit(main())
