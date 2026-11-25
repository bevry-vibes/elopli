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

## follow-up prompt

linux and windows would be good, as I use both, they could share the same base - we have cross-platform menubar/systray stuff in bevry-vibes/aural-keyboard - would a zig base help that then swift uses on macos, or does swift now work suitably enough on all platforms?

two other functioanlity that the official app provides that we don't yet, but should be added for completeness:
- rename
- Power On Behaviour: [Restore Last Used Setting] or [Restore Defaults]

## follow-up prompt 2

I agree rust then is better than zig. So our base is either our of rust or swift.

Swift does have open-source extensions that fill the lack of cross-platform ui - compare the swift open-source ui options with the rust options for windows and fedora, and the native options (that would consume the rust core) for windows and fedora.

The requirements are all must except for apple notarisation.

Note that unlike aural-keyboard, our interface will need to be a proper interface and not just a menubar combobox - so we will need a ui that has cards for each light.

Note the official elgato app also has wifi icons which when hovered over indicate wifi connection, wifi signal strength, and wifi frequency - is this provided by the light? or via macos?

## follow-up prompt 3

okay, swiftcrossui llm policy neutralised our ability to do upstream tray: https://github.com/moreSwift/swift-cross-ui/blob/main/LLM_POLICY.md - as I do not have the inclination to deviate my intention from getting this done, to becoming an informed associate of their codebase

so do rust core, with per-platform native uis
- for windows, is ther enot winui optiosn for rust, whatever other options are there for language, or is c# the best option?
- for linux, doesn't linux have it's own recommended programming language for gtk?
compare their recommended languages, with other supported languages

the cost of independent uis is minimal as our ui is simple, and it is a good learning repo for future for cross-platform native work

okay, just do appimage for now, shelf flatpak for future

for code signing, what are the costs for signing?

appincidactor on gnome is acceptible, we already use it in aural-keyboard or whatever right?
