import datetime
import multiprocessing
from time import sleep
import yaml
import threading

from src.celestial_body import Body
from src.timeframe import TimeFrame
from src.visualizer import Visualizer, Visualizer2D

class Interplanetary:
    def __init__(self, body_path) -> None:
        f = open(body_path, 'r')
        bodies = yaml.load(f, Loader=yaml.Loader)
        f.close()
        
        self.planets = []
        self.create_planetes(bodies)
        
        self.tfnow = TimeFrame()
        self.steps = []
        self.realtime_lock = multiprocessing.Lock()

    def create_planetes(self, bodies):
        if "objects" in bodies:
            obj = bodies["objects"]
            for name, values in obj.items():
                self.planets.append(
                    Body(name, values["body"], values["j2000"])
                )

    def simulate(self, timesteps_range):
        self.realtime_lock.acquire()
        for planet in self.planets:
            planet.compute_orbit(timesteps_range)
        self.realtime_lock.release()

    def run(self, step_size=-1):
        self.shouldRun = True

        visualizer = Visualizer()
        queue = multiprocessing.Queue(maxsize=1)
        visualizer.set_update_rate_and_feed(25, queue, self.realtime_lock)

        self.realtime_thread = threading.Thread(target=self.realtime, args=(queue, step_size))
        self.realtime_thread.daemon = True
        self.realtime_thread.start()

        visualizer.start()

    def stop(self):
        if self.realtime_thread and self.shouldRun:
            self.shouldRun = False
            self.realtime_thread.join()

    def realtime(self, queue, step_size):
        while self.shouldRun:
            if step_size == -1:
                self.tfnow = TimeFrame()
                sleep(1)
            else:
                self.tfnow.add_dd(datetime.timedelta(seconds=step_size))
            self.steps.append(self.tfnow.copy())
            self.steps = self.steps[-50:]
            self.simulate(self.steps)
            queue.put(self.planets)
            # print(self.planets[0].name, self.planets[0].get_path())

    def plotgl(self, xyz=False):
        if xyz:
            visualizer = Visualizer()
        else:
            visualizer = Visualizer2D()
        visualizer.plot(self.planets)
        visualizer.start()