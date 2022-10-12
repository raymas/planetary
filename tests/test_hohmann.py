from sqlite3 import Time
import yaml
from src.celestial_body import Body, Spacecraft
from src.hohmann import Hohmann
from src.timeframe import TimeFrame

def test_terra_mars():
    h = Hohmann()

    f = open("./res/objects.yaml", 'r')
    bodies = yaml.load(f, Loader=yaml.Loader)
    f.close()

    obj = bodies["objects"]
    earthy = obj["earth"]
    earth = Body("earth", earthy["body"], earthy["j2000"])

    marsy = obj["mars"]
    mars = Body("mars", marsy["body"], marsy["j2000"])

    s = Spacecraft(100e3)
    t = TimeFrame()

    comp = h.transfer_planetary(earth, mars, s, 75e3, t)

    assert comp["dv"] == 5684