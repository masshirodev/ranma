# Port of src/whichkey.rs layout, extended with folders; used to draw the mocks.
import sys
NAME_MAX=16
W=len
def cut(n): return n if W(n)<=NAME_MAX else n[:NAME_MAX-1]+'…'
# row: (key, name, kind) kind in 'bind','folder','empty'
def gh(g): return 1+len(g[1])
def col_of(gs):
    kw=nw=hw=0
    for name,rows in gs:
        for r in rows:
            kw=max(kw,W(r[0])); nw=max(nw,min(W(r[1]),NAME_MAX))
        hw=max(hw,W(name))
    h=sum(gh(g) for g in gs)+max(len(gs)-1,0)
    return dict(groups=gs,kw=kw,w=max(kw+1+nw,hw),h=h)
def pack(groups,h,avail):
    cols=[];total=0
    for i,g in enumerate(groups):
        if gh(g)>h: return cols,total,i
        if cols:
            last=cols[-1]; c=col_of(last['groups']+[g])
            if c['h']<=h and total-last['w']+c['w']<=avail:
                total=total-last['w']+c['w']; cols[-1]=c; continue
        c=col_of([g]); nt=total+(0 if not cols else 2)+c['w']
        if nt>avail: return cols,total,i
        total=nt; cols.append(c)
    return cols,total,len(groups)
def keyp(P,x,y,key):
    i=key.rfind('+')
    if i>0 and i+1<len(key):
        P.append((x,y,key[:i+1],'M')); P.append((x+i+1,y,key[i+1:],'K'))
    else: P.append((x,y,key,'K'))
NAMEROLE={'bind':'S','folder':'D','empty':'E'}
def layout(groups,sw,sh,crumbs,help='?',back=False):
    if not groups: return None
    first=groups[0]; cap=max(sh//2-2,0); avail=max(sw-4,0); P=[]
    if cap>=gh(first) and avail>=col_of([first])['w']:
        packed=pack(groups,gh(first),avail)
        for h in range(gh(first),cap+1):
            packed=pack(groups,h,avail)
            if packed[2]==len(groups): break
        cols,total,kept=packed
        used=max([c['h'] for c in cols] or [0])
        pw,ph=max(total,28)+4,used+2
        cx=2
        for c in cols:
            cy=1
            for name,rows in c['groups']:
                P.append((cx,cy,name,'H'))
                for i,r in enumerate(rows):
                    y=cy+1+i
                    keyp(P,cx+c['kw']-W(r[0]),y,r[0])
                    P.append((cx+c['kw']+1,y,cut(r[1]),NAMEROLE[r[2]]))
                cy+=gh((name,rows))+1
            cx+=c['w']+2
        dropped=[g[0] for g in groups[kept:]]
        frame(P,pw,ph,crumbs,help,dropped,back)
        return dict(w=pw,h=ph,P=P,flowed=False,dropped=dropped)
    if sw<30 or cap<1: return None
    longest=max(len(g[1]) for g in groups)
    flat=[g[1][i] for i in range(longest) for g in groups if i<len(g[1])]
    lines=[[]];x=0
    for r in flat:
        w=W(r[0])+1+W(r[1])
        if x>0 and x+2+w>avail:
            if len(lines)==cap: break
            lines.append([]);x=0
        if x>0: x+=2
        lines[-1].append((x,r)); x+=w
    for y,line in enumerate(lines):
        for x,r in line:
            keyp(P,2+x,1+y,r[0]); P.append((2+x+W(r[0])+1,1+y,r[1],NAMEROLE[r[2]]))
    ph=len(lines)+2
    frame(P,sw,ph,crumbs,help,[],back)
    return dict(w=sw,h=ph,P=P,flowed=True,dropped=[])
def frame(P,pw,ph,crumbs,help,dropped,back):
    # top: the keys pressed so far, joined by ›; cut from the left with …
    room=pw-4
    parts=list(crumbs)
    def tw(ps): return sum(W(p) for p in ps)+3*(len(ps)-1)
    lead=False
    while tw(parts)+(2 if lead else 0)>room and len(parts)>1:
        parts.pop(0); lead=True
    x=1; P.append((x,0,' ','S')); x+=1
    if lead: P.append((x,0,'… ','S')); x+=2
    for i,p in enumerate(parts):
        if i: P.append((x,0,' › ','S')); x+=3
        P.append((x,0,p,'K')); x+=W(p)
    P.append((x,0,' ','S'))
    b=ph-1
    hw=W(help)+11 if help else 0
    fx=max(pw-(hw+2),0)
    if help:
        P.append((fx,b,' ','S')); P.append((fx+1,b,help,'K')); P.append((fx+1+W(help),b,' all keys ','S'))
    x=1
    if back:
        P.append((1,b,' ','S')); P.append((2,b,'bksp','K')); P.append((6,b,' back ','S')); x=11
        if dropped: P.append((x,b,'·','S')); x+=1
    if dropped:
        t=' +'+' · '.join(dropped)+' '
        rm=max(fx-1-x,0)
        P.append((x,b,t[:rm],'S'))
def draw(p,border=True):
    w,h=p['w'],p['h']
    g=[[' ']*w for _ in range(h)]; r=[['F' if border else 'S']*w for _ in range(h)]
    if border:
        g[0]=['─']*w; g[h-1]=['─']*w
        for row in g: row[0]='│'; row[w-1]='│'
        g[0][0],g[0][w-1],g[h-1][0],g[h-1][w-1]='╭','╮','╰','╯'
        for y in range(1,h-1):
            for x in range(1,w-1): r[y][x]='S'
    for x,y,t,role in p['P']:
        for i,ch in enumerate(t):
            if x+i<w: g[y][x+i]=ch; r[y][x+i]=role
    return g,r

# The default binds' rows, as src/whichkey.rs groups them. Redrawn 2026-10-09
# when the defaults gained settings, the tmux extras and two folders.
D_LAYOUT=('layout',[('←↓↑→','focus','bind'),('shift+←↓↑→','resize','bind'),('ctrl+shift+←↓↑→','move','bind'),('alt+←↓↑→','split there','bind'),('j','split dir','bind'),('=','equalize','bind'),('alt+⏎','fullscreen','bind'),('M','swap master','bind'),('spc','next layout','bind')])
D_STRIP=('strip',[('←↓↑→','focus','bind'),('shift+←↓↑→','resize','bind'),('ctrl+shift+←↓↑→','move','bind'),('alt+←↓↑→','split there','bind'),('{ }','join/leave','bind'),('e','width','bind'),('E','full width','bind'),('c','centre','bind'),('=','equalize','bind'),('alt+⏎','fullscreen','bind')])
D_PANES=('panes',[('t','new pane','bind'),('q','close','bind'),('w','float','bind'),('f','next float','bind'),('g','group','bind'),('ctrl+h/l','prev/next tab','bind'),(',','rename','bind'),('i','pane numbers','bind'),('x','+pane','folder')])
D_WS=('workspaces',[('1-0','workspace','bind'),('alt+1-0','send pane','bind'),('ctrl+←→','prev/next','bind'),('ctrl+↓','empty one','bind'),('.','rename','bind'),('s','scratchpad','bind'),('alt+s','to scratchpad','bind'),(';','last pane','bind'),('l','last one','bind')])
D_SESS=('sessions',[('tab','pane list','bind'),('bksp','session list','bind'),('N','new session','bind'),('( )','prev/next','bind'),('$','rename','bind'),('m','move workspace','bind')])
D_HIST=('history',[('/','search','bind'),('[','copy mode','bind'),('o','links','bind'),('v','paste image','bind'),('V','copies','bind'),(']','paste last','bind')])
D_RANMA=('ranma',[(':','commands','bind'),('p','settings','bind'),('r','reload config','bind'),('d','detach','bind'),('S','servers','bind'),('U','update','bind'),('Del','quit','bind'),('esc','leave WM','bind'),('y','+layouts','folder')])
# The default folders, opened.
F_PANE=[('pane',[('a','sync input','bind'),('A','unsync all','bind'),('c','centre','bind'),('l','log pane','bind'),('m','menu','bind'),('r','respawn','bind'),('s','watch silence','bind'),('z','size 80%','bind'),('←↓↑→','snap half','bind')])]
F_LAYOUTS=[('layouts',[('l','load layout','bind'),('s','save layout','bind')])]
DEFAULTS=[D_LAYOUT,D_PANES,D_WS,D_SESS,D_HIST,D_RANMA]
STRIP=[D_STRIP,D_PANES,D_WS,D_SESS,D_HIST,D_RANMA]

def text(p,border=True):
    g,_=draw(p,border); return [''.join(r) for r in g]
def check():
    src=open('/home/claude/ranma/doc/handoffs/done/WHICH_KEY_MOCK.txt').read()
    for name,sw,sh,b in [('80x24 rounded',80,24,True),('80x24 none',80,24,False),('120x35 rounded',120,35,True),('200x50 rounded',200,50,True),('40x15 flowed',40,15,True)]:
        want=[l for l in src.split('## ') if l.startswith(name)][0].splitlines()[1:]
        got=text(layout(DEFAULTS,sw,sh,['ctrl+b']),b)
        print(name, 'OK' if got==want else 'DIFF')
        if got!=want:
            for a,c in zip(got,want):
                if a!=c: print(repr(a));print(repr(c))
if __name__=='__main__': check()
