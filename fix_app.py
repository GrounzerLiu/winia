import re

with open('winia/src/app.rs','r',encoding='utf-8') as f:
    content = f.read()

# Remove ModifiersChanged + keyboard dispatch handler
# Find the sections by their unique content markers

# Remove: ModifiersChanged handler + dispatch_key_event block
# Start at "WindowEvent::ModifiersChanged(m)" end at the Tab handler
old_start = 'WindowEvent::ModifiersChanged(m) => {'
old_end = 'WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {'

if old_start in content:
    start_idx = content.index(old_start)
    end_idx = content.index(old_end, start_idx)
    
    # Find the end of the KeyboardInput handler
    # It ends at the next WindowEvent:: or closing
    remaining = content[end_idx:]
    # Find next 'WindowEvent::' or '}' that closes this section
    depth = 0
    handler_end = end_idx
    for i in range(len(remaining)):
        c = remaining[i]
        if c == '{': depth += 1
        elif c == '}': depth -= 1
        if depth < 0:
            handler_end = end_idx + i + 1
            break
    
    # Remove from ModifiersChanged to end of KeyboardInput
    content = content[:start_idx] + content[handler_end:] if start_idx < handler_end else content
    
    # Actually let me just do a simpler replacement
    content = content.replace(old_start, '// REMOVED')

with open('winia/src/app.rs','w',encoding='utf-8') as f:
    f.write(content)

print('done')
