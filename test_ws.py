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
        # 看根 Column 的 size（节点 1）
        matches = re.findall(r'"pos":\[([-\d.]+),([-\d.]+)\],"size":\[([-\d.]+),([-\d.]+)\]', tree)
        if len(matches) >= 2:
            m = matches[1]
            print(f'Scroll Column: size=({m[2]},{m[3]})')
            print(f'Height matches window 680? {abs(float(m[2])-648)<1}')
            print(f'Width matches window 680? {abs(float(m[2])-648)<1}')
        # 看看有没有 f32::MAX
        for m in matches:
            if 'e' in m[3] or 'E' in m[3]:
                print(f'  INFINITE: {m}')
        print('No infinite sizes!' if not any('e' in m[3] for m in matches) else 'INFINITE FOUND!')

asyncio.run(t())
