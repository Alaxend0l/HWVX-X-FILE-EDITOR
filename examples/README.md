# Examples

## `demo-project/` — vendor-first rebuild (works today)

Rebuilds the PC `files` archive from your own game extract, dropping
world pack 2 and re-inserting one edited interface pack. It needs two
folders that are **not** checked in:

```sh
cd examples/demo-project

# 1. Vendor base: extract the game archive once (path to your orig copy)
hwvx-x extract ../../orig/pc/files vendor/pc

# 2. A pack to re-insert: extract just the interface pack folder you want
#    to edit, drop it under mywork/, and edit its sections/pack.json.
#    Folder names like `04_interface_0` carry the key, so the project
#    needs no explicit type/id.
cp -r vendor/pc/04_interface_0 mywork/04_interface_0

# 3. Build (default target: pc -> build/files.xpc)
hwvx-x build .

# Both containers at once
hwvx-x build . --target pc --target pc-xnc
```

Everything the build did is reproducible from `hwvx.project.json`: the
vendor packs come from `vendor/pc` minus `"world:2"`, then
`mywork/04_interface_0` is appended. Try `--target pc:xnc` too — same
build, `.xnc` container, no project changes.

The other example kinds (authored cars, tracks) appear as milestones
land — see [docs/compiler/roadmap.md](../docs/compiler/roadmap.md).
