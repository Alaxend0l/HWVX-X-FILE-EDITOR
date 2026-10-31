# tiny helpers for exploring extracted packs
import struct, json, sys, os
class P:
    def __init__(s, d, e):
        s.d=d; s.e=e
        s.blob=open(d+"/data.bin","rb").read()
        s.ptrs=set()
        if os.path.exists(d+"/pointers.bin"):
            pb=open(d+"/pointers.bin","rb").read()
            s.ptrs=set(struct.unpack(e+f"{len(pb)//4}I",pb))
        s.json=json.load(open(d+"/pack.json"))
        def tab(n,w):
            p=d+f"/{n}.bin"
            if not os.path.exists(p): return []
            b=open(p,"rb").read(); return [struct.unpack_from(e+"2I",b,i) for i in range(0,len(b),8)]
        s.tex=tab("texture_relocs",8); s.refr=tab("ref_relocs",8); s.snd=tab("sound_relocs",8)
    def u32(s,o): return struct.unpack_from(s.e+"I",s.blob,o)[0]
    def f32(s,o): return struct.unpack_from(s.e+"f",s.blob,o)[0]
    def dump(s,o,n=16):
        for a in range(o,o+n*4,4):
            w=s.u32(a); f=s.f32(a)
            raw=s.blob[a:a+4]; t="".join(chr(c) if 32<=c<127 else "." for c in raw)
            print(f"  {a:06X}+{a-o:03X}: {w:08X} {'PTR' if a in s.ptrs else '   '} {f:<14.6g} {t}")
