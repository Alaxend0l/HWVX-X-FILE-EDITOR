#!/usr/bin/env python3
"""Read a PS2 memory card image (PCSX2 .ps2, raw 8 MB card with or without ECC).

  ps2_mcd.py CARD                 list every directory and file
  ps2_mcd.py CARD PATH OUT        extract one file (PATH like /BASLUS-20412/xyz)

The card file system ("Sony PS2 Memory Card Format 1.2"): a superblock in page 0
(page size, pages per cluster, clusters per card, the allocatable area's first
cluster, the root directory's cluster, the indirect FAT cluster list), a FAT of
u32 entries (bit 31: in use; low 31 bits: next cluster; 0x7FFFFFFF: last) found
through two levels of indirection, and 512-byte directory entries (mode, length,
first cluster, name). Data cluster numbers are relative to the allocatable area;
FAT and indirect cluster numbers are absolute. Raw images carry 16 ECC bytes
after each 512-byte page, which are skipped.
"""
import struct
import sys


class Card:
    def __init__(self, path):
        self.d = open(path, 'rb').read()
        sb = self.d[:0x154]
        if not sb.startswith(b'Sony PS2 Memory Card Format '):
            raise ValueError('not a PS2 memory card image')
        (self.page_len, self.ppc, self.ppb) = struct.unpack_from('<HHH', sb, 0x28)
        (self.clusters, self.alloc_offset, self.alloc_end, self.root) = struct.unpack_from('<IIII', sb, 0x30)
        self.ifc = [c for c in struct.unpack_from('<32I', sb, 0x50) if c]
        pages = len(self.d) // self.page_len
        # raw images store ECC after each page
        self.raw_page = self.page_len + 16 if len(self.d) % (self.page_len + 16) == 0 and \
            len(self.d) // (self.page_len + 16) * self.page_len == self.clusters * self.ppc * self.page_len \
            else self.page_len
        self.cluster_len = self.page_len * self.ppc
        self.fat = self._read_fat()

    def page(self, n):
        o = n * self.raw_page
        return self.d[o:o + self.page_len]

    def cluster(self, n):
        return b''.join(self.page(n * self.ppc + i) for i in range(self.ppc))

    def _read_fat(self):
        fat = []
        per = self.cluster_len // 4
        for ic in self.ifc:
            for fc in struct.unpack('<%dI' % per, self.cluster(ic)):
                if fc in (0, 0xFFFFFFFF):
                    continue
                fat.extend(struct.unpack('<%dI' % per, self.cluster(fc)))
        return fat

    def chain(self, first):
        c, seen = first, set()
        while c != 0x7FFFFFFF and c not in seen and c < len(self.fat):
            seen.add(c)
            yield c
            e = self.fat[c]
            if not e & 0x80000000:
                break
            c = e & 0x7FFFFFFF

    def read(self, first, length):
        data = b''.join(self.cluster(self.alloc_offset + c) for c in self.chain(first))
        return data[:length]

    def entries(self, first, count):
        raw = self.read(first, count * 512)
        for i in range(count):
            e = raw[i * 512:(i + 1) * 512]
            mode, length = struct.unpack_from('<HxxI', e, 0)
            cluster = struct.unpack_from('<I', e, 0x10)[0]
            mtime = struct.unpack_from('<xBBBBBH', e, 0x18)   # sec min hour day month year
            name = e[0x40:0x60].split(b'\0')[0].decode('latin1')
            yield dict(mode=mode, length=length, cluster=cluster, name=name,
                       mtime='%04d-%02d-%02d %02d:%02d:%02d' % (mtime[5], mtime[4], mtime[3], mtime[2], mtime[1], mtime[0]))

    def walk(self, first=None, count=None, path=''):
        if first is None:
            root = next(self.entries(self.root, 1))
            first, count = self.root, root['length']
        for e in self.entries(first, count):
            if not e['mode'] & 0x8000 or e['name'] in ('.', '..'):
                continue
            p = path + '/' + e['name']
            yield p, e
            if e['mode'] & 0x20:
                yield from self.walk(e['cluster'], e['length'], p)

    def find(self, path):
        for p, e in self.walk():
            if p == path:
                return e
        return None


def main():
    card = Card(sys.argv[1])
    if len(sys.argv) == 2:
        for p, e in card.walk():
            kind = 'dir ' if e['mode'] & 0x20 else 'file'
            print(f"{kind} {e['length']:8d}  {e['mtime']}  {p}")
        return
    e = card.find(sys.argv[2])
    if e is None or e['mode'] & 0x20:
        sys.exit('no such file: ' + sys.argv[2])
    open(sys.argv[3], 'wb').write(card.read(e['cluster'], e['length']))


if __name__ == '__main__':
    main()
