# survey TileCar vehicle / port / maxVehicle in every extracted world pack
import glob, struct, sys, collections
sys.path.insert(0, '..')
from xp import P
summ = collections.Counter()
for d in sorted([g for g in glob.glob('worlds/*/*_world_*') if 'texture' not in g]):
    p = P(d, '>'); root = 0
    n = p.u32(root+8); cars = p.u32(root+0xC)
    rows = []
    for i in range(n):
        c = cars + i*0x60
        veh = struct.unpack_from('>i', p.blob, c+0x30)[0]
        port = p.blob[c+0x35]; mx = p.blob[c+0x37]
        rows.append('%d/p%d/m%d' % (veh, port, mx))
        summ[(veh if veh in (-1,) else 'fixed', mx)] += 1
    print(d.split('/')[1], n, ' '.join(rows))
print(summ)
