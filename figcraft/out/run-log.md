# FigCraft headless run (Ubuntu runner, no display, no FUSE)

The binary driven here is `appdir/usr/bin/figcraft-cli`, shipped inside the
published `figcraft-0.18.1-linux-x86_64.AppImage` and unpacked with
`--appimage-extract` (checksum verified against the release SHA256SUMS.txt).
No window, no GPU: the engine's CPU renderer does the drawing.

## AppImage payload
```
total 211936
-rwxr-xr-x 1 runner runner 125844248 Nov 12 00:50 figcraft
-rwxr-xr-x 1 runner runner  91175576 Nov 12 00:50 figcraft-cli
```

## Version
```
figcraft-cli 0.18.1
```

## Command surface
```
645 commands exposed
  node.create        yes  Create layer
  node.set           yes  Set properties
  text.set           yes  Set text
  component.create   yes  Create component
  layout.add         yes  Add auto layout
  inspect.code       yes  Code for layer
  inspect.properties yes  Inspect properties
  file.export        yes  Export
```

## Design script -> undo/confinement-checked run, then reopen + edit
```
{"command":"node.create","ok":true,"result":{"id":2},"step":0}
{"command":"node.create","ok":true,"result":{"id":3},"step":1}
{"command":"node.create","ok":true,"result":{"id":4},"step":2}
{"command":"node.create","ok":true,"result":{"id":5},"step":3}
{"command":"node.create","ok":true,"result":{"id":6},"step":4}
{"command":"node.create","ok":true,"result":{"id":7},"step":5}
{"command":"node.create","ok":true,"result":{"id":8},"step":6}
{"command":"node.create","ok":true,"result":{"id":9},"step":7}
{"command":"component.create","ok":true,"result":{"id":8},"step":8}
{"command":"node.create","ok":true,"result":{"id":10},"step":9}
{"command":"page.rename","ok":true,"result":{"ok":true},"step":10}

{"command":"command.batch","ok":true,"result":[{"command":"text.set","ok":true,"result":{"ok":true},"step":0},{"command":"node.set","ok":true,"result":{"ok":true},"step":1},{"command":"node.set","ok":true,"result":{"ok":true},"step":2},{"command":"inspect.code","ok":true,"result":{"code":"import \"./sign-in.css\";\n\nexport function SignIn() {\n  return (\n    <div className=\"sign-in\">\n      <div className=\"ellipse\"></div>\n      <p className=\"welcome-back\">Hello again</p>\n      <p className=\"sign-in-to-the-figcraft-design-system\">Sign in to the FigCraft design system</p>\n      <div className=\"email-field\">\n        <p className=\"you-studio-dev\">you@studio.dev</p>\n      </div>\n      <div className=\"continue\">\n        <p className=\"continue-2\">Continue</p>\n      </div>\n      <p className=\"no-account-start-a-14-day-trial\">No account? Start a 14-day trial</p>\n    </div>\n  );\n}\n\n/* sign-in.css */\n.sign-in {\n  position: relative;\n  width: 390px;\n  height: 844px;\n  display: flex;\n  flex-direction: column;\n  gap: 20px;\n  padding: 32px 32px 32px 32px;\n  justify-content: center;\n  align-items: center;\n  background: #FFF7F0;\n}\n.ellipse {\n  width: 72px;\n  height: 72px;\n  border-radius: 50%;\n  background: #7048E8;\n}\n.welcome-back {\n  color: #1A1523;\n  font-family: \"Inter\";\n  font-size: 30px;\n  font-weight: 700;\n}\n.sign-in-to-the-figcraft-design-system {\n  color: #6B6480;\n  font-family: \"Inter\";\n  font-size: 15px;\n  font-weight: 400;\n}\n.email-field {\n  align-self: stretch;\n  width: 326px;\n  height: 100px;\n  display: flex;\n  flex-direction: column;\n  gap: 0px;\n  padding: 16px 16px 16px 16px;\n  justify-content: flex-start;\n  align-items: flex-start;\n  border-radius: 12px;\n  background: #FFFFFF;\n  box-shadow: 0px 2px 10px 0px rgba(26, 21, 35, 0.12);\n}\n.you-studio-dev {\n  color: #6B6480;\n  font-family: \"Inter\";\n  font-size: 16px;\n  font-weight: 400;\n}\n.continue {\n  align-self: stretch;\n  width: 326px;\n  height: 100px;\n  display: flex;\n  flex-direction: row;\n  gap: 0px;\n  padding: 16px 16px 16px 16px;\n  justify-content: center;\n  align-items: flex-start;\n  border-radius: 999px;\n  background: #1098AD;\n}\n.continue-2 {\n  color: #FFFFFF;\n  font-family: \"Inter\";\n  font-size: 16px;\n  font-weight: 700;\n}\n.no-account-start-a-14-day-trial {\n  color: #8B849C;\n  font-family: \"Inter\";\n  font-size: 13px;\n  font-weight: 400;\n}\n","connected":false,"language":"react"},"step":3},{"command":"inspect.code","ok":true,"result":{"code":"/* Sign in */\nwidth: 390px;\nheight: 844px;\ndisplay: flex;\nflex-direction: column;\ngap: 20px;\npadding: 32px 32px 32px 32px;\njustify-content: center;\nalign-items: center;\nbackground: #FFF7F0;\n","connected":false,"language":"css"},"step":4},{"command":"inspect.properties","ok":true,"result":{"sections":[{"rows":[{"label":"Width","value":"326"},{"label":"Height","value":"100"},{"label":"Top","value":"507"},{"label":"Left","value":"32"},{"label":"Radius","value":"999"}],"title":"Layout"},{"rows":[{"label":"Direction","value":"Horizontal"},{"label":"Gap","value":"0"},{"label":"Padding","value":"16"},{"label":"Align","value":"center, start"}],"title":"Auto layout"},{"rows":[{"color":"#1098AD","label":"Solid","value":"rgb(16, 152, 173)"}],"title":"Fills"}]},"step":5}],"step":0}
{"command":"node.create","ok":true,"result":{"id":11},"step":1}
```

## MCP server (headless engine over stdio JSON-RPC)
```
initialize: {"name": "figcraft", "title": "FigCraft", "version": "0.18.1"}
tools/list: 19 tools
  batch, create_node, export, get_node, inspect_document, inspect_ui, key, list_commands, open_file, pointer, redo, run_command, save_file, screenshot, select, set_properties, type_text, undo, wait_for_changes
create_node frame -> 2 isError: False
create_node text -> isError: False
batch -> isError: False
set_properties -> isError: False
inspect_document -> 1394 bytes
screenshot -> ['/home/runner/work/testtemp/testtemp/figcraft/out/mcp-card.png']
undo ok
checks: {"create_frame": true, "create_text": true, "batch": true, "set_properties": true, "inspect_document": true, "screenshot": true}
MCP SMOKE: PASS
```

## File stats
```
{
  "file": "figcraft/out/sign-in.figcraft",
  "images": 0,
  "missingFonts": [],
  "name": "sign-in",
  "nodes": 10,
  "notes": [],
  "pages": [
    {
      "frames": [
        {
          "height": 844.0,
          "id": 2,
          "name": "Sign in",
          "type": "FRAME",
          "width": 390.0
        }
      ],
      "id": 1,
      "name": "Onboarding",
      "nodes": 9,
      "topLevel": 1
    }
  ],
  "styles": 0,
  "variables": 0
}
figcraft/out/sign-in-v2.figcraft — sign-in (11 nodes, 0 styles, 0 variables, 0 images)
page 1 "Onboarding": 10 nodes, 1 top-level
  2 FRAME "Sign in" 390.0×844.0
```
