import subprocess,pathlib
from PIL import Image
import imageio_ffmpeg
O=pathlib.Path('deliverables/immersive'); C='.local/deckcraft/squashfs-root/usr/bin/deckcraft-cli'
F=O/'frames';F.mkdir(exist_ok=True)
fps=24; count=40
for s in range(1,4):
 subprocess.run([C,'render-transition',str(O/'immersive.pptx'),'--slide',str(s),'--t',','.join(f'{i/count:.3f}' for i in range(1,count+1)),'--width','1280',str(F/f's{s}.png')],check=True,stdout=subprocess.DEVNULL)
ff=imageio_ffmpeg.get_ffmpeg_exe()
p=subprocess.Popen([ff,'-y','-f','rawvideo','-pix_fmt','rgb24','-s','1280x720','-r',str(fps),'-i','-','-an','-c:v','libx264','-crf','19','-pix_fmt','yuv420p','-movflags','+faststart',str(O/'immersive.mp4')],stdin=subprocess.PIPE,stderr=open(O/'encode.log','w'))
def frame(path,n=1):
 im=Image.open(path).convert('RGB').resize((1280,720));b=im.tobytes()
 for _ in range(n):p.stdin.write(b)
for s in range(4):
 frame(O/'preview'/f'slide-{s+1:02}.png',77)
 if s<3:
  for i in range(1,count+1):frame(F/f's{s+1}-t{i/count:.3f}.png')
p.stdin.close()
assert p.wait()==0
print('Created',O/'immersive.mp4')
