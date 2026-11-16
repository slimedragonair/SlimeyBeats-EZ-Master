"""In-memory identities; the owner remains the default for trusted store callers."""

from dataclasses import dataclass
import math


@dataclass(frozen=True)
class Principal:
    id: str
    expires: float = math.inf
    deadline: float = math.inf

    @property
    def is_owner(self):
        return self.id == "owner"


OWNER = Principal("owner")
