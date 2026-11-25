# prompts

Model: OpenCode · DeepSeek V4.1 Flash (`opencode-go/deepseek-v4.1-flash`)

Serial numbers are redacted in this companion, per the instruction in the prompt.

## original prompt

I have `/Applications/Elgato Control Center.app` it use to provide 'Apple Shortcuts' actions, but an update a year ago disabled/removed that functionality, and support does not care (hence it being a year later).

Reverse engineer their API to the key lights, of which there are three:

Nickname/name: Back
Device: Key Light Air
Firmware: 1.0.3 (222)
Serial: <redacted>
IP: 192.168.4.36

Nickname/name: Left
Device: Key Light
Firmware: 1.0.3 (222)
Serial: <redacted>
IP: 192.168.4.37

Nickname/name: Right
Device: Key Light
Firwmare: 1.0.3 (222)
Serial: <redacted>
IP: 192.168.4.39

Treat the serials as information to be redacted.

The properties that need the ability to change are warmth, brightness, on/off.

Start with on/off.

Provide a CLI.

Scan github and whatnot for existing reverse engeered options, or anything else that can help.

Use github.com/bevry-vibes/skills to scaffold this project.

Ping me back once you have cracked the API interface, and we'll propose a plan for which language to code the official client in (swift, rust, zig are good contenders). You can do your exploration in whatever.
