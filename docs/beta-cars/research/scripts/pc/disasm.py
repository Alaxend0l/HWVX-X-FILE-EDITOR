import pefile, capstone, sys
pe=pefile.PE(sys.argv[1]); base=pe.OPTIONAL_HEADER.ImageBase
t=[s for s in pe.sections if s.Name.startswith(b'.text')][0]
data=t.get_data(); va=base+t.VirtualAddress
md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32); md.skipdata=True
for i in md.disasm(data,va):
    print("%08x %06x %-20s %s %s"%(i.address,i.address-va+t.PointerToRawData,i.bytes.hex(),i.mnemonic,i.op_str))
