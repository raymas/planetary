# Planetary utils

This repository is a planetary showcase for approximate planet position

## What can I find ?

- Orbit drawing based on J2000 orbital elements.
- Hohmann transfer computation for spacecraft

### Orbit drawing

![3d](showcase/visualizer3d.gif)

All celestial bodies are defined in [objects.yaml](objects.yaml) file. Data are extracted from the Jet Lab Propulsion website

### Hohmann transfer

Work in progress

- [X] dV required for A to B transfer
- [X] Period of transfer
- [ ] Window size in second

## References

[ssd.jpl.nasa.gov](https://ssd.jpl.nasa.gov/planets/approx_pos.html)

[www.stjarnhimlen.se](https://www.stjarnhimlen.se/comp/ppcomp.html)

[www.projectrho.com](http://www.projectrho.com/public_html/rocket/mission.php)
