import sys
import numpy as np
import pyqtgraph as pg
import pyqtgraph.opengl as gl

from pyqtgraph.Qt import QtGui, QtCore

from src.celestial_body import Body

class Visualizer:

    scale = 1 / 1e6

    def __init__(self) -> None:
        self.app = QtGui.QApplication(sys.argv)
        self.w = gl.GLViewWidget()
        self.w.opts['distance'] = 1e6
        self.w.setWindowTitle('Planetary viewer')
        self.w.resize(1000, 1000)
        self.w.show()
        
        self.graphmap = {}

    def start(self) -> None:
        if (sys.flags.interactive != 1) or not hasattr(QtCore, 'PYQT_VERSION'):
            QtGui.QApplication.instance().exec_()

    def make_mesh(self, planet: Body) -> gl.GLMeshItem:
        radius = planet.radius * Visualizer.scale
        color = (planet.color[0] / 255, planet.color[1] / 255, planet.color[2] / 255, 1.)
        planet_mesh = gl.MeshData.sphere(32, 32, radius=radius)
        planet_gl = gl.GLMeshItem(meshdata=planet_mesh, smooth=False, color=color)
        return planet_gl

    def plot(self, planets: list) -> None:
        for planet in planets:
            path = planet.get_path() * Visualizer.scale
            color = planet.color
            orbit = gl.GLLinePlotItem(pos=path, color=pg.glColor((color[0], color[1], color[2], 255)), width=1., antialias=True)
            self.w.addItem(orbit)

            planet_gl = self.make_mesh(planet)
            planet_gl.translate(*(path[-1]))
            self.w.addItem(planet_gl)

            text = pg.TextItem(text='Hello')
            self.w.addItem(text)

    def set_update_rate_and_feed(self, rate, queue, lock) -> None:
        self.rate = rate
        self.queue = queue
        self.lock = lock

        self.timer = QtCore.QTimer()
        self.timer.timeout.connect(self.update)
        self.timer.start(self.rate)

    def update(self):
        data = self.queue.get()
        
        self.lock.acquire()
        for planet in data:
            path = planet.get_path() * Visualizer.scale
            path = path[~np.all(path == 0, axis=1)] # TODO: race condition here ?
            if path.size == 0:
                continue
            # print(planet.name, path)
            color = planet.color

            if not planet.name in self.graphmap:
                self.graphmap[planet.name] = {}
                self.graphmap[planet.name]["orbit"] = gl.GLLinePlotItem(pos=[], color=pg.glColor((color[0], color[1], color[2], 255)), width=1., antialias=True)
                self.graphmap[planet.name]["planet_gl"] = self.make_mesh(planet)
                self.w.addItem(self.graphmap[planet.name]["orbit"])
                self.w.addItem(self.graphmap[planet.name]["planet_gl"])

            self.graphmap[planet.name]["orbit"].setData(pos=path)
            self.graphmap[planet.name]["planet_gl"].resetTransform()
            self.graphmap[planet.name]["planet_gl"].translate(*(path[-1]))
        self.lock.release()


class Visualizer2D:
    def __init__(self) -> None:
        self.app = QtGui.QApplication(sys.argv)
        self.w = pg.GraphicsLayoutWidget(show=True, title="Planetary")
        self.w.show()

        self.x = self.w.addPlot(title="Position (x plane)")
        self.y = self.w.addPlot(title="Position (y plane)")
        self.z = self.w.addPlot(title="Position (z plane)")

        self.w.nextRow()

        self.xy = self.w.addPlot(title="Position (xy plane)")
        self.yz = self.w.addPlot(title="Position (yz plane)")

        self.graphmap = {}

    def start(self) -> None:
        if (sys.flags.interactive != 1) or not hasattr(QtCore, 'PYQT_VERSION'):
            QtGui.QApplication.instance().exec_()
        
    def plot(self, planets) -> None:
        for planet in planets:
            path = planet.get_path()
            pathxy = path[:,0:2]
            pathyz = path[:,1:3]

            color = planet.color
            self.xy.plot(pathxy, pen=color, name=planet.name)
            self.yz.plot(pathyz, pen=color, name=planet.name)

            self.x.plot(path[:,0], pen=color, name=planet.name)
            self.y.plot(path[:,1], pen=color, name=planet.name)
            self.z.plot(path[:,2], pen=color, name=planet.name)

    def set_update_rate_and_feed(self, rate, queue) -> None:
        self.rate = rate
        self.queue = queue

        self.timer = QtCore.QTimer()
        self.timer.timeout.connect(self.update)
        self.timer.start(self.rate)

    def update(self):
        data = self.queue.get()

        for planet in data:
            if not planet.name in self.graphmap:
                self.graphmap[planet.name] = {}
                self.graphmap[planet.name]["xyp"] = self.xy.plot([], symbol='o')
                self.graphmap[planet.name]["yzp"] = self.yz.plot([], symbol='o')
                self.graphmap[planet.name]["xp"] = self.x.plot([])
                self.graphmap[planet.name]["yp"] = self.y.plot([])
                self.graphmap[planet.name]["zp"] = self.z.plot([])

            path = planet.get_path()
            pathxy = path[:,0:2]
            pathyz = path[:,1:3]

            color = planet.color

            graphs = self.graphmap[planet.name]

            graphs["xyp"].setData(pathxy, pen=color)
            graphs["yzp"].setData(pathyz, pen=color)

            graphs["xp"].setData(path[:,0], pen=color)
            graphs["yp"].setData(path[:,1], pen=color)
            graphs["zp"].setData(path[:,2], pen=color)

