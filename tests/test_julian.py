import datetime
from src.timeframe import TimeFrame

def test_timereference():
    tests = [
        {'year': 2022, 'month': 7, 'day': 31, 'hours': 0, 'minutes': 0, 'seconds': 0, 'microseconds': 0, 'expected': 2459791.5000000},
        {'year': 2022, 'month': 7, 'day': 31, 'hours': 12, 'minutes': 12, 'seconds': 12, 'microseconds': int(580e3), 'expected': 2459792.0084789},
    ]

    for test in tests:
        d = datetime.datetime(test['year'], test['month'], test['day'], test['hours'], test['minutes'], test['seconds'], test['microseconds'], datetime.timezone.utc)
        t = TimeFrame()
        t.set_date(d)

        ref = (test['expected'] - 2451545.0) / 36525

        assert t.get_d() == ref

    