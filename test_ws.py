import asyncio, websockets, re, sys, json

async def t():
    for i in range(30):
        try:
            async with websockets.connect('ws://localhost:9998', ping_interval=None) as w: break
        except: await asyncio.sleep(0.4)
    else: print('timeout'); sys.exit(1)
    async with websockets.connect('ws://localhost:9998', ping_interval=None) as w:
        await asyncio.sleep(1)
        await w.send('t'); tree = await asyncio.wait_for(w.recv(), timeout=5)
        # 找所有 pos/size
        matches = re.findall(r'"pos":\[([-\d.]+),([-\d.]+)\],"size":\[([-\d.]+),([-\d.]+)\]', tree)
        # 找最后一个 RichText 附近的大 size
        for pos, m in enumerate(matches):
            x,y,w,h = float(m[0]),float(m[1]),float(m[2]),float(m[3])
            if h > 50:
                print(f'  [{pos}] pos=({x:.0f},{y:.0f}) size=({w:.0f},{h:.0f})')
        # 找最后一个节点的 size
        if matches:
            last = matches[-1]
            print(f'Last node: pos=({last[0]},{last[1]}) size=({last[2]},{last[3]})')

asyncio.run(t())
