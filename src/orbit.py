import numpy as np
from src.timeframe import TimeFrame

class OrbitParameter:
    def __init__(self, parameters: object) -> None:
        self.value = parameters["value"]
        self.rate = parameters["rate"]

    def get(self, t: TimeFrame):
        return self.value + self.rate * t.get_d()

class Orbit:
    def __init__(self, parameters) -> None:
        self.a = OrbitParameter(parameters["a"])
        self.e = OrbitParameter(parameters["e"])
        self.I = OrbitParameter(parameters["I"])
        self.L = OrbitParameter(parameters["L"])
        self.w = OrbitParameter(parameters["w"])
        self.W = OrbitParameter(parameters["W"])
        self.dm = parameters["daily_motion"]

    def compute_orbit(self, t: TimeFrame) -> np.array:
        # https://ssd.jpl.nasa.gov/planets/approx_pos.html
        a = self.a.get(t) * 149597870700
        e = self.e.get(t)

        w = np.deg2rad(self.w.get(t))
        W = np.deg2rad(self.W.get(t))
        L = np.deg2rad(self.L.get(t))
        I = np.deg2rad(self.I.get(t))

        o = w - W

        T = t.get_d()
        M = (L - w) + self.dm["b"] * T * T + self.dm["c"] * np.cos(self.dm["f"] * T) + self.dm["s"] * np.sin(self.dm["f"] * T)

        dE = 1e99
        it = 0
        E = M - e * np.sin(M)
        tol = np.deg2rad(1e-6)
        while np.abs(dE) > tol:
            dM = M - (E - e * np.sin(E))
            dE = dM / (1 - e * np.cos(E))
            E = E + dE
            it += 1

            if it > 1e9:
                raise ValueError("Cannot find a solution : " + str(dE))

        xh = a * (np.cos(E) - e)
        yh = a * np.sqrt(1.0 - pow(e, 2)) * np.sin(E)

        v = np.arctan2(yh, xh)
        r = np.sqrt(xh*xh + yh*yh)

        xecl = r * ( np.cos(o) * np.cos(v + W) - np.sin(o) * np.sin(v+W) * np.cos(I) )
        yecl = r * ( np.sin(o) * np.cos(v+W) + np.cos(o) * np.sin(v+W) * np.cos(I) )
        zecl = r * ( np.sin(v+W) * np.sin(I) )

        # J2000 frame
        # eps = np.rad2deg(23.43928)
        # xeq = xecl
        # yeq = xecl + np.cos(eps) * yecl - np.sin(eps) * zecl
        # zeq = xecl + np.sin(eps) * yecl + np.cos(eps) * zecl

        return np.array([xecl, yecl, zecl])
