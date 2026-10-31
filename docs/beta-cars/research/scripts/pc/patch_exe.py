#!/usr/bin/env python3
"""Patch a COPY of the PC "Hot Wheels.exe" (sha1 91af8432d6903371f724c4c2924c4ba51cc9eb73)
so car ids 33..35 (0x21..0x23) count as unlocked.

  GameSettings_GetCodeVariable (VA 0x42A680), case 0x64, game type 0:
      if (arg < 0x21) return cars[arg];
      + if (arg < 0x24) return 1;                       (cave A, VA 0x429E51)
  GameSettings_SetCodeVariable (VA 0x429FA0), case 0x64, game type 0:
      if (arg < 0x21) cars[arg] = val != 0;
      + else if (arg < 0x24) pad5A[arg - 0x21] = val != 0;   (cave B, VA 0x42B8F1)

Caves are 15-byte NOP alignment runs after a `ret` (no code jumps into them, no
base relocations in them). All new code is position independent (rel32 only).

usage: patch_exe.py IN.exe OUT.exe [--no-set]
"""
import hashlib, sys

ORIG_SHA1 = '91af8432d6903371f724c4c2924c4ba51cc9eb73'
IMAGE = 0x400000
TEXT_VA, TEXT_RAW = 0x401000, 0x1000


def off(va):
    return va - TEXT_VA + TEXT_RAW


def rel32(src_next, dst):
    return (dst - src_next).to_bytes(4, 'little', signed=True)


# --- get(0x64): cave A -------------------------------------------------------
CAVE_A = 0x429E51
GET_JAE = 0x42A707          # jae 0x42a829 (return 0) after cmp eax,0x21
RET_ONE = 0x42A6CF          # pop edi; pop esi; pop ebp; mov eax,1; pop ebx; ret
RET_ZERO = 0x42A829         # xor eax,eax; pop edi; pop esi; pop ebp; pop ebx; ret
cave_a = (bytes.fromhex('83f824')                                   # cmp eax,0x24
          + b'\x0f\x82' + rel32(CAVE_A + 9, RET_ONE)                # jb  RET_ONE
          + b'\xe9' + rel32(CAVE_A + 14, RET_ZERO))                 # jmp RET_ZERO

# --- set(0x64): cave B -------------------------------------------------------
CAVE_B = 0x42B8F1
SET_JAE = 0x42A00F          # jae 0x42a4f4 (pop esi; ret) after cmp eax,0x21
SET_STORE = 0x42A015        # mov ecx,[esp+0x10]; test; seta dl; mov [eax+0x64d300],dl; pop esi; ret
cave_b = (bytes.fromhex('83f824')                                   # cmp eax,0x24
          + bytes.fromhex('7308')                                   # jae +8 -> pop esi; ret
          + bytes.fromhex('83c039')                                 # add eax,0x39 (0x21 -> 0x5A = pad5A)
          + b'\xe9' + rel32(CAVE_B + 13, SET_STORE)                 # jmp SET_STORE
          + bytes.fromhex('5ec3'))                                  # pop esi; ret

NOP15 = b'\x90' * 15
PATCHES_GET = [
    ('get: jae -> cave A', off(GET_JAE), bytes.fromhex('0f831c010000'), b'\x0f\x83' + rel32(GET_JAE + 6, CAVE_A)),
    ('get: cave A', off(CAVE_A), NOP15[:len(cave_a)], cave_a),
]
PATCHES_SET = [
    ('set: jae -> cave B', off(SET_JAE), bytes.fromhex('0f83df040000'), b'\x0f\x83' + rel32(SET_JAE + 6, CAVE_B)),
    ('set: cave B', off(CAVE_B), NOP15[:len(cave_b)], cave_b),
]


def main():
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    if len(args) != 2:
        sys.exit(__doc__)
    src, dst = args
    data = bytearray(open(src, 'rb').read())
    h = hashlib.sha1(data).hexdigest()
    if h != ORIG_SHA1:
        print('warning: input sha1 %s is not the retail exe; checking bytes anyway' % h)
    patches = PATCHES_GET + ([] if '--no-set' in sys.argv else PATCHES_SET)
    for name, o, old, new in patches:
        assert len(old) == len(new), name
        cur = bytes(data[o:o + len(old)])
        if cur == new:
            print('%-20s @0x%06x already applied' % (name, o))
            continue
        if cur != old:
            sys.exit('%s @0x%06x: expected %s, found %s; not patching' % (name, o, old.hex(), cur.hex()))
        data[o:o + len(new)] = new
        print('%-20s @0x%06x %s -> %s' % (name, o, old.hex(), new.hex()))
    for va in (CAVE_A + 15, CAVE_B + 15):   # caves must end right before a function
        assert data[off(va) - 1] in (0x90, 0xC3), hex(va)
    if src == dst:
        sys.exit('refusing to overwrite the input; give a different output path')
    open(dst, 'wb').write(data)
    print('wrote %s (sha1 %s)' % (dst, hashlib.sha1(data).hexdigest()))
    try:
        import capstone
        md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
        for name, o, old, new in patches:
            va = o - TEXT_RAW + TEXT_VA
            for i in md.disasm(bytes(data[o:o + len(new)]), va):
                print('   %08x  %-14s %s %s' % (i.address, i.bytes.hex(), i.mnemonic, i.op_str))
    except ImportError:
        pass


if __name__ == '__main__':
    main()
