"""Build actual Morph slides with DeckCraft, then render transitions to an MP4."""
import json,subprocess,pathlib
O=pathlib.Path('deliverables/immersive'); O.mkdir(exist_ok=True,parents=True)
CLI=str(pathlib.Path('.local/deckcraft/squashfs-root/usr/bin/deckcraft-cli').resolve())
def run(cmds,src=None,save=None):
 args=[CLI,'run']
 if src: args+=['--in',str(src)]
 for n,k in cmds: args+=['--cmd',n+'='+json.dumps(k)]
 if save: args+=['--save',str(save)]
 return subprocess.check_output(args+['--print'],text=True)
c=[]
def add(n,**k): c.append((n,k))
def shape(name,preset,rect,color):
 add('shape.insert',preset=preset,rect=rect);add('shape.rename',name=name);add('shape.fill',color=color);add('shape.line',none=True)
def txt(name,rect,text,size,color):
 add('insert.textBox',rect=rect,text=text);add('shape.rename',name=name);add('format.font',family='DejaVu Sans');add('format.size',size=size);add('format.color',color=color);add('format.bold',on=True)
add('file.new',blank=True,size=[960,540]);add('slide.new',layout='blank');add('design.background',color='#080F20')
shape('orbit','ellipse',[520,-80,620,620],'#112E46');shape('core','ellipse',[650,130,220,220],'#40E2BF');shape('satellite','ellipse',[565,375,45,45],'#FFB86C')
txt('eyebrow',[48,35,850,30],'A STORY IN MOTION',13,'#40E2BF')
txt('headline',[48,150,590,150],'Start with\none bright idea.',46,'#F4F7FF')
txt('caption',[48,332,490,86],'Give your audience a single point\nto follow. Then let the story unfold.',20,'#AABCD2')
txt('footer',[48,482,850,30],'01 / DISCOVER                                      DECKCRAFT • MORPH STUDY',11,'#7E96B1')
add('transition.timing',onClick=False,after=3200)
p=O/'immersive.deckcraft';run(c,save=p)
stages=[
 ('Connect the dots.','Bring ideas into relationship.\nBuild a path, not a wall of information.','02 / CONNECT',{'orbit':[200,-120,700,700],'core':[390,210,150,150],'satellite':[730,280,80,80]},[48,105,800,100],[48,402,850,65]),
 ('Make the idea\nfeel bigger.','Scale creates emphasis.\nMotion creates continuity.','03 / EXPAND',{'orbit':[-240,-180,1000,1000],'core':[500,35,500,500],'satellite':[415,420,28,28]},[48,135,490,150],[48,335,440,85]),
 ('Leave a clear\nlasting impression.','One story. A deliberate rhythm.\nA confident finish.','04 / REMEMBER',{'orbit':[380,-70,650,650],'core':[700,155,180,180],'satellite':[620,355,48,48]},[48,130,650,150],[48,335,550,90])]
for headline,caption,footer,positions,hr,cr in stages:
 run([('slide.last',{}),('slide.duplicate',{})],src=p,save=p)
 result=run([('slide.last',{}),('slide.inspect',{})],src=p)
 data=json.loads(result.split('slide.inspect → ',1)[1]); shapes=data['shapes']; ids={s['name']:s['id'] for s in shapes}
 cmds=[('slide.last',{})]
 for name,rect in {**positions,'headline':hr,'caption':cr}.items():
  cmds += [('edit.select',{'ids':[ids[name]]}),('shape.sizePosition',dict(zip(['x','y','w','h'],rect)))]
 for name,t in [('headline',headline),('caption',caption),('footer',footer+'                                      DECKCRAFT • MORPH STUDY')]:cmds.append(('text.set',{'id':ids[name],'text':t}))
 cmds += [('transition.set',{'kind':'morph','duration':1600}),('transition.timing',{'onClick':False,'after':3200})]
 run(cmds,src=p,save=p)
subprocess.run([CLI,'convert',str(p),str(O/'immersive.pptx')],check=True)
subprocess.run([CLI,'render',str(O/'immersive.pptx'),'--all','--scale','1',str(O/'preview')],check=True)
