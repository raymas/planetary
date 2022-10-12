import datetime
import os
import numpy as np
from src.celestial_body import Spacecraft
from src.hohmann import Hohmann

from src.interplanetary import Interplanetary
from src.timeframe import TimeFrame

def main():
    objects = os.path.join(os.path.dirname(__file__), "res", "objects.yaml")
    interplanetary = Interplanetary(objects)

    # h = Hohmann()
    # s = Spacecraft(300e3)
    # print(h.transfer_planetary(interplanetary.planets[3], interplanetary.planets[4], s, 300e3, TimeFrame()))

    # real time
    interplanetary.run(step_size=60*60*24)

    # simulated time
    # ref = datetime.datetime.now()
    # tfs = []
    # for i in range(-10 * 365, 0, 1):
    #     tf = TimeFrame()
    #     offset = ref + datetime.timedelta(days=i)
    #     tf.set_date(offset)
    #     tfs.append(tf)
    # interplanetary.simulate(tfs)
    # interplanetary.plotgl(True)

if __name__ == '__main__':
    main()