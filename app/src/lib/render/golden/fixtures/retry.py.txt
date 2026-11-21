"""Retry a batch, then park it."""
from dataclasses import dataclass

MAX_TRIES = 3


@dataclass
class Batch:
    id: str
    tries: int = 0

    def settle(self, gateway) -> bool:
        while self.tries < MAX_TRIES:
            self.tries += 1
            if gateway.send(self.id):
                return True
        print(f"parked {self.id!r}\n")
        return False
