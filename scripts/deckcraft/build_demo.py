"""Generate a reproducible DeckCraft command script, not a substitute PPTX writer."""
import json
from pathlib import Path
out=Path('deliverables/deckcraft-demo'); out.mkdir(parents=True,exist_ok=True)
cmd=[]
def c(name,**kw): cmd.append(name+'='+json.dumps(kw))
def box(x,y,w,h,color):
 c('shape.insert',preset='rect',rect=[x,y,w,h]); c('shape.fill',color=color); c('shape.line',none=True)
def text(x,y,w,h,t,size=24,color='#F4F6FC',bold=False):
 c('insert.textBox',rect=[x,y,w,h],text=t); c('format.font',family='DejaVu Sans'); c('format.size',size=size); c('format.color',color=color); c('format.bold',on=bold)
def slide(n,label):
 if n==1: c('file.new',blank=True,size=[960,540]); c('slide.new',layout='blank')
 else: c('slide.new',layout='blank')
 c('design.background',color='#101B30'); box(48,43,36,4,'#49E0BE'); text(96,29,800,32,label,13,'#49E0BE',True)
 text(48,491,800,25,'DECKCRAFT  /  HEADLESS DEMONSTRATION',10,'#92A2BC'); text(864,485,60,30,f'{n:02}',14,'#92A2BC')
slide(1,'FROM BRIEF TO BOARDROOM')
text(48,126,850,150,'A clearer story.\nA stronger presentation.',48,bold=True)
text(48,302,720,76,'A custom slide deck created with the real\nDeckCraft command-line engine.',23,'#B7C5DA')
box(48,421,280,3,'#49E0BE'); text(48,438,800,30,'EDITABLE POWERPOINT  /  NO GUI REQUIRED',12,'#49E0BE')
c('slide.notes',text='Demonstration only. This deck illustrates a design and automation workflow, not business performance data.')
slide(2,'01 / THE DESIGN PRINCIPLES')
text(48,89,850,70,'Clarity is a design decision.',36,bold=True)
for x,num,title,body in [(48,'01','One idea','Make the takeaway\nimpossible to miss.'),(344,'02','Clear hierarchy','Use scale and space\nto guide attention.'),(640,'03','Useful visuals','Show relationships,\nnot decoration.')]:
 box(x,207,272,221,'#1B2B44'); text(x+20,222,230,50,num,30,'#49E0BE',True); text(x+20,288,240,40,title,23,bold=True); text(x+20,345,240,70,body,18,'#B7C5DA')
slide(3,'02 / A REPEATABLE WORKFLOW')
text(48,89,850,70,'Build deliberately. Review visually.',36,bold=True)
for i,(a,b) in enumerate([('Brief','Audience + outcome'),('Structure','Argument + evidence'),('Design','Layout + typography'),('Validate','Render + inspect')]):
 x=48+i*222; box(x,222,200,5,'#49E0BE'); text(x,251,205,50,a,25,bold=True); text(x,310,205,65,b,17,'#B7C5DA'); text(x,391,200,40,f'0{i+1}',20,'#49E0BE')
c('slide.notes',text='Before delivery, check exported slides in the intended presentation application. Rendering here is performed by DeckCraft; Microsoft PowerPoint compatibility must be checked separately.')
slide(4,'03 / READY FOR YOUR CONTENT')
text(48,104,850,110,'Your story comes next.',44,bold=True)
text(48,231,800,145,'Send the topic and audience.\nAdd your brand, source material and slide count.\nThen we can build a focused, tailored deck.',25,'#B7C5DA')
box(48,418,864,3,'#49E0BE'); text(48,438,850,35,'BRIEF  →  STORY  →  DESIGN  →  REVIEW',16,'#49E0BE',True)
c('file.properties',title='From Brief to Boardroom',subject='DeckCraft headless capability demonstration',author='Arena')
(out/'commands.txt').write_text('\n'.join(cmd)+'\n')
