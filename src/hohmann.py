from numbers import Number
import datetime
import numpy as np
from src.celestial_body import Body, Spacecraft
from src.timeframe import TimeFrame 

# http://www.projectrho.com/public_html/rocket/mission.php#id--Hohmann_Transfer_Orbits
# http://www.phy6.org/stargaze/Smap.htm

class Hohmann:
    def __init__(self) -> None:
        pass

    def transfer_planetary(self, ref: Body, target: Body, spaceship: Spacecraft, target_altitude: Number, t: TimeFrame) -> list:
        """Compute dv and required time to reach target body from reference body"""

        # reference computation on sun
        µp = 1.989e30 * 6.67430e-11
        tord = target.orbit.a.get(t) * 149597870700
        rord = ref.orbit.a.get(t) * 149597870700
        sma = (rord + tord) / 2.0
        ovs = np.sqrt(µp / rord)
        vs = np.sqrt(µp * ((2 / rord) - (1 / sma)))
        vis = np.abs(vs - ovs)

        # using sun as pivot
        µs = ref.mass * 6.67430e-11
        por = ref.radius + spaceship.r
        povs = np.sqrt(μs / por)
        ves = np.sqrt((2 * μs) / por)
        vhs = np.sqrt(vis * vis + ves * ves)
        dvs = vhs - povs

        # print(por, povs, ves, vhs, dvs)

        ovd = np.sqrt(µp / tord)
        vd = np.sqrt(µp * ((2 / tord) - (1 / sma)))
        vid = np.abs(vd - ovd)
        μt = target.mass * 6.67430e-11
        pord = target.radius + target_altitude
        pocv = np.sqrt(μt / pord)
        vycd = np.sqrt((2 * μt) / pord)
        vhd = np.sqrt(vid * vid + vycd * vycd)
        dvd = vhd - pocv

        # delta velocity
        dV = np.abs(dvs) + np.abs(dvd)

        # duration of journey
        T = .5 * np.sqrt(2 * np.pi * np.pi * sma * sma * sma / µp)

        # window size in second 
        wref = 2 * np.pi * np.sqrt(pow(rord, 3) / µp)
        wtar = 2 * np.pi * np.sqrt(pow(tord, 3) / µp)
        w = 1 / ((1 / wref) - (1 / wtar))
        wt = datetime.datetime.now() + datetime.timedelta(seconds=w)
        
        alpha = np.pi * (1 - ((1/(2*np.sqrt(2))) * np.sqrt(pow(rord/tord + 1, 3))))

        return {
            "dV": dV,
            "T": T,
            "w": wt
        }