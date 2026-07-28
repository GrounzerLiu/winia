import asyncio, websockets, re, sys

async def t():
    for i in range(20):
        try:
            async with websockets.connect('ws://localhost:9998', ping_interval=None) as w: break
        except: await asyncio.sleep(0.4)
    else: print('timeout'); sys.exit(1)
    async with websockets.connect('ws://localhost:9998', ping_interval=None) as w:
        await asyncio.sleep(1)
        await w.send('t'); tree = await asyncio.wait_for(w.recv(), timeout=5)
        # 找 Decorations 段落的 RichSpanStyle
        spans = re.findall(r'"spans":\[([^\]]{10,300})\]', tree)
        for s in spans[:3]:
            print(f'span: {s[:200]}')

asyncio.run(t())
