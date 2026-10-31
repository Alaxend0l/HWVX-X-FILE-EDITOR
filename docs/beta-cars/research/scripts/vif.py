import sys, struct
sys.path.insert(0, 'SCRATCH')
from xp import P
UNP = {0x0:('S',32,1),0x1:('S',16,1),0x2:('S',8,1),0x4:('V2',32,2),0x5:('V2',16,2),0x6:('V2',8,2),0x8:('V3',32,3),0x9:('V3',16,3),0xA:('V3',8,3),0xC:('V4',32,4),0xD:('V4',16,4),0xE:('V4',8,4),0xF:('V4-5',5,1)}
def walk(B, o, nbytes, verbose=True):
    end = o + nbytes; out = []
    while o < end:
        w = struct.unpack_from('<I', B, o)[0]; o += 4
        cmd = (w >> 24) & 0x7F; num = (w >> 16) & 0xFF; imm = w & 0xFFFF
        if cmd >= 0x60:
            vn = (cmd >> 2) & 3; vl = cmd & 3; fmt = cmd & 0xF
            kind, bits, comps = UNP[fmt]
            n = num or 256
            size = ((bits * comps * n + 31) // 32) * 4 if fmt != 0xF else ((16*n+31)//32)*4
            out.append(('UNPACK', kind + '-' + str(bits), n, imm & 0x3FF, bool(imm & 0x8000), bool(cmd & 0x10), o, size))
            o += size
        elif cmd == 0x50 or cmd == 0x51:
            n = imm or 65536
            out.append(('DIRECT' if cmd == 0x50 else 'DIRECTHL', n, o)); o += 16*n
        elif cmd in (0x20,):
            out.append(('STMASK', struct.unpack_from('<I', B, o)[0])); o += 4
        elif cmd == 0x30 or cmd == 0x31:
            out.append(('STROW' if cmd == 0x30 else 'STCOL', struct.unpack_from('<4I', B, o))); o += 16
        elif cmd == 0x4A:
            n = num or 256
            out.append(('MPG', n, imm)); o += 8*n
        else:
            names = {0:'NOP',1:'STCYCL',2:'OFFSET',3:'BASE',4:'ITOP',5:'STMOD',6:'MSKPATH3',7:'MARK',0x10:'FLUSHE',0x11:'FLUSH',0x13:'FLUSHA',0x14:'MSCAL',0x15:'MSCALF',0x17:'MSCNT'}
            out.append((names.get(cmd, 'CMD%02X' % cmd), imm, num, w))
    return out
if __name__ == '__main__':
    p = P(sys.argv[1], '<'); o = int(sys.argv[2], 16); n = int(sys.argv[3], 16)
    for c in walk(p.blob, o, n): print(c)
