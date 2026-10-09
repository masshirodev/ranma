import html,sys
from wk import *
B=lambda k,n:(k,n,'bind'); Fo=lambda k,n:(k,'+'+n,'folder')
D_PANES_U=('panes',[r for r in D_PANES[1] if r[0]!='g'])
D_HIST_V=('history',D_HIST[1]+[('v','paste image','bind')])
PLUGINS=('plugins',[B('h','history'),B('n','notes'),Fo('i','agents')])
YOURS=('yours',[B('c','editor'),B('e','files'),Fo('g','git')])
TOP=[D_LAYOUT,D_PANES_U,D_WS,PLUGINS,YOURS,D_SESS,D_HIST_V,D_RANMA]
GIT=[('git',[B('c','commit'),B('d','diff'),B('l','log'),B('p','push'),B('P','pull'),B('s','status'),Fo('b','branches')])]
BR=[('branches',[B('b','switch'),B('d','delete'),B('m','merge'),B('n','new')])]
EMPTY_TOP=[D_LAYOUT,D_PANES_U,D_WS,('plugins',[B('h','history'),B('n','notes'),Fo('i','agents'),('x','+scratch','empty')]),YOURS,D_SESS,D_HIST_V,D_RANMA]
EMPTY=[('scratch',[('','nothing bound','empty')])]
LEFT=['~/src/ranma $ cargo test whichkey','   Compiling ranma v0.31.0','    Finished test profile in 6.2s','running 7 tests','test tests::short_spellings ... ok','test tests::at_80x24_three_groups ... ok','test tests::at_120x35_history ... ok','test tests::at_200x50_every_group ... ok','test tests::folders_open_in_place ... ok','test result: ok. 7 passed; 0 failed','~/src/ranma $ ']
RIGHT=['~/notes $ ls','inbox.md  ranma.md  todo.md','~/notes $ ']
def screen(sw,sh,panel,border=True):
    g=[[' ']*sw for _ in range(sh)]; r=[['t']*sw for _ in range(sh)]
    ph=sh-1; lw=sw//2
    def box(x0,w,title,content,cls):
        for y in range(ph):
            for x in range(x0,x0+w):
                ch=' '
                if y==0: ch='─'
                elif y==ph-1: ch='─'
                if x==x0 or x==x0+w-1: ch='│' if 0<y<ph-1 else ch
                g[y][x]=ch
                if ch!=' ': r[y][x]=cls
        g[0][x0]='╭';g[0][x0+w-1]='╮';g[ph-1][x0]='╰';g[ph-1][x0+w-1]='╯'
        t=' '+title+' '
        for i,ch in enumerate(t[:w-4]): g[0][x0+1+i]=ch; r[0][x0+1+i]=cls
        for j,line in enumerate(content):
            if j+1>=ph-1: break
            for i,ch in enumerate(line[:w-4]): g[j+1][x0+2+i]=ch; r[j+1][x0+2+i]='t'
    box(0,lw,'~/src/ranma',LEFT,'bA'); box(lw,sw-lw,'notes',RIGHT,'bI')
    # bar
    y=sh-1
    def put(x,s,c):
        for i,ch in enumerate(s):
            if x+i<sw: g[y][x+i]=ch; r[y][x+i]=c
        return x+len(s)
    x=put(0,' WM ','W'); x=put(x,'  ','t'); x=put(x,'1:zsh','Ba'); x=put(x,'  2:nvim  3:ai','t')
    clock='Fri 09 Oct  05:56 '
    if sw>=60: put(sw-len(clock),clock,'t')
    if panel:
        pg,pr=draw(panel,border); py=sh-1-panel['h']
        for j in range(panel['h']):
            for i in range(panel['w']):
                g[py+j][i]=pg[j][i]; r[py+j][i]=pr[j][i]
    return g,r
def tohtml(g,r):
    out=[]
    for row,cls in zip(g,r):
        s='';cur=None;buf=''
        def flush():
            nonlocal s
            if buf: s+= html.escape(buf,quote=False) if cur=='t' else f'<span class="{cur}">{html.escape(buf,quote=False)}</span>'
        for ch,c in zip(row,cls):
            if c!=cur: flush(); cur=c; buf=''
            buf+=ch
        flush()
        out.append(f'<div>{s}</div>')
    return '\n'.join(out)
CR=['ctrl+b']
boards={
 'top80': screen(80,24,layout(TOP,80,24,CR)),
 'git80': screen(80,24,layout(GIT,80,24,CR+['g'],back=True)),
 'br80': screen(80,24,layout(BR,80,24,CR+['g','b'],back=True)),
 'git80none': screen(80,24,layout(GIT,80,24,CR+['g'],back=True),False),
 'top120': screen(120,35,layout(TOP,120,35,CR)),
 'git120': screen(120,35,layout(GIT,120,35,CR+['g'],back=True)),
 'top200': screen(200,50,layout(TOP,200,50,CR)),
 'git40': screen(40,15,layout(GIT,40,15,CR+['g'],back=True)),
 'top40': screen(40,15,layout(TOP,40,15,CR)),
 'emptytop': screen(120,35,layout(EMPTY_TOP,120,35,CR)),
 'empty80': screen(80,24,layout(EMPTY,80,24,CR+['x'],back=True)),
 'def80': screen(80,24,layout(DEFAULTS,80,24,CR)),
}
if __name__=='__main__':
    for k,(g,r) in boards.items():
        print('##',k); print('\n'.join(''.join(x) for x in g))
        open(f'out_{k}.html','w').write(tohtml(g,r))
