import numpy as np

def test_accuracy():
    x = 1./3.
    for i in range(30):
        x = (9. * x + 1) * x - 1
        print(i, x)
    assert x == 1/3

def test_accuracy_np():
    x = np.float128(1./3.)
    for i in range(30):
        x = (9. * x + 1) * x - 1
        print(i, x)
    assert x == 1/3