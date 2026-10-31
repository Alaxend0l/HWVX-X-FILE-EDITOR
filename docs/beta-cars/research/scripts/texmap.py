"""Order-preserving alignment of shared-pack textures beta -> GC by thumbnail
distance (Needleman-Wunsch), printed with the distance and dimensions."""
import sys, json
sys.path.insert(0, __file__.rsplit("/", 1)[0])
import numpy as np
from texdecode import ps2_textures, gc_textures, thumb
def dist(a, b):
    ta, tb = thumb(a[1]), thumb(b[1])
    if ta is None or tb is None: return 200.0
    d = np.abs(ta[..., :3] - tb[..., :3]).mean() + 0.5 * np.abs(ta[..., 3] - tb[..., 3]).mean()
    if (a[0][1], a[0][2]) != (b[0][1], b[0][2]): d += 5     # size changed
    return float(d)
def align(A, B, gap=30.0):
    n, m = len(A), len(B)
    D = np.array([[dist(a, b) for b in B] for a in A])
    C = np.zeros((n + 1, m + 1)); P = np.zeros((n + 1, m + 1), int)
    C[:, 0] = np.arange(n + 1) * gap; C[0, :] = np.arange(m + 1) * gap
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            opts = (C[i-1, j-1] + D[i-1, j-1], C[i-1, j] + gap, C[i, j-1] + gap)
            k = int(np.argmin(opts)); C[i, j] = opts[k]; P[i, j] = k
    i, j, mp = n, m, {}
    while i and j:
        k = P[i, j]
        if k == 0: mp[i-1] = (j-1, D[i-1, j-1]); i -= 1; j -= 1
        elif k == 1: i -= 1
        else: j -= 1
    return mp, D
if __name__ == "__main__":
    A = ps2_textures(sys.argv[1]); B = gc_textures(sys.argv[2])
    mp, D = align(A, B)
    used = set(int(x) for x in sys.argv[3].split(",")) if len(sys.argv) > 3 else set(range(len(A)))
    res = {}
    for i in range(len(A)):
        best = int(np.argmin(D[i]))
        if i in mp:
            j, d = mp[i]
            flag = "" if d < 15 else ("  CHECK" if d < 40 else "  NO MATCH?")
            if best != j and D[i, best] + 3 < d: flag += f" (closest elsewhere: gc {best} d={D[i,best]:.1f})"
            print(f"{'*' if i in used else ' '} beta {i:3d} {A[i][0]} -> gc {j:3d} {B[j][0]} d={d:5.1f}{flag}")
            res[i] = j
        else:
            print(f"{'*' if i in used else ' '} beta {i:3d} {A[i][0]} -> (gap)  closest gc {best} d={D[i,best]:.1f}")
    json.dump(res, open(sys.argv[4] if len(sys.argv) > 4 else "/dev/null", "w"))
