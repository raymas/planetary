import hashlib
import numpy as np
from src.orbit import Orbit

class Body:
    """Main body class parameters"""
    def __init__(self, name, parameters, orbital_parameters) -> None:
        self.name = name

        self.mass = float(parameters["mass"])
        self.radius = float(parameters["diameter"]) / 2.0
        self.orbit = Orbit(orbital_parameters)
        self.path = np.zeros((1, 3))

        self.color = parameters["color"]

    def color(self):
        return self.color

    def generate_color(self):
        hh = hashlib.blake2s()
        hh.update(bytearray(self.name, "utf-8"))
        ret = hh.digest()
        return (ret[0], ret[1], ret[2])

    def compute_orbit(self, timesteps):
        self.path = np.zeros((len(timesteps), 3), dtype=np.float64)
        for i, t in enumerate(timesteps):
            self.path[i,:] = self.orbit.compute_orbit(t).copy()

    def get_path(self) -> np.array:
        return self.path.copy()

    def get_position(self) -> np.array:
        return self.path[-1].copy()

    def __repr__(self) -> str:
        return self.name

    def __str__(self) -> str:
        return self.name

class Spacecraft:

    def __init__(self, r) -> None:
        self.r = r

    