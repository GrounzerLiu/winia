import asyncio, websockets, re, sys

async def t():
    for i in range(20):
        try:
            async with websockets.connect('ws://localhost:9998', ping_interval=None) as w: break
        except: await asyncio.sleep(0.3)
    else: print('timeout'); sys.exit(1)
    async with websockets.connect('ws://localhost:9998', ping_interval=None) as w:
        await asyncio.sleep(2)
        await w.send('t'); tree = await asyncio.wait_for(w.recv(), timeout=5)
        print(tree[:500])
asyncio.run(t())
