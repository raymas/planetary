import datetime

class TimeFrame:
    """Datetime to date"""
    def __init__(self) -> None:
        self.now = datetime.datetime.now(datetime.timezone.utc)
        self.compute_d(self.now)
        self.j2000 = 0

    def add_dd(self, value: datetime.datetime):
        self.now = self.now + value
        self.compute_d(self.now)

    def set_date(self, date: datetime.datetime) -> None:
        self.now = date
        self.compute_d(self.now)

    def compute_d(self, ref: datetime.datetime) -> None:
        cal = 'g'
        t = self.floating_hour(ref) / 24
        dy = ref.day + t
        mnth = ref.month
        yr = ref.year

        if mnth == 1 or mnth == 2:
            mnth += 12
            yr = yr - 1
        
        if yr <= 1582:
            cal = 'j'
            if yr == 1582 and mnth >= 10:
                cal = 'g'
        
        A = 0
        B = 0

        if cal == 'g':
            A = int(yr/100)
            B = 2 - A + int(A/4)

        p1 = int(365.25*(yr + 4716))
        p2 = int(30.6001*(mnth + 1))
        
        self.j2000 = p1 + p2 + dy + B - 1524.5
        self.d = (self.j2000 - 2451545.0) / 36525.0

    def floating_hour(self, ref: datetime.datetime) -> float:
        return ref.hour + ref.minute / 60 + ref.second / 3600 # + ref.microsecond / 1e6

    def get_d(self):
        return self.d

    def copy(self):
        t = TimeFrame()
        t.set_date(self.now)
        return t