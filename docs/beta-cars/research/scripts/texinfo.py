import struct, json
def texheaders(d, beta):
    blob = open(d + "/data.bin", "rb").read(); h = json.load(open(d + "/pack.json"))["header"]
    o = h["texture_data_offset"]; out = []
    for i in range(h["num_textures"]):
        if beta:
            f, w, hh, m = struct.unpack_from("<4H", blob, o + 0x10 * i); out.append((f, w, hh, m))
        else:
            f, w, hh, m = struct.unpack_from("<4H", blob, o); out.append((f, w, hh, m))
            pix = struct.unpack_from(">I", blob, o + 8)[0]
            # next header: 0x10 header padded to 0x20, then pixels? headers are contiguous in GC too?
            o += 0x20
    return out
if __name__ == "__main__":
    import sys
    for i, t in enumerate(texheaders(sys.argv[1], "beta" in sys.argv[1])): print(i, t)
