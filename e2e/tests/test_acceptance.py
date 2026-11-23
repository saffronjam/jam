import pytest

from jam_e2e.cases import CASES
from jam_e2e.steps import execute


@pytest.fixture(params=CASES, ids=lambda case: case.id)
def case(request):
    return request.param


def test_user_journey(jam, case):
    memory = {}
    for index, step in enumerate(case.steps, 1):
        try:
            execute(jam, step, memory)
        except Exception as error:
            raise AssertionError(f"{case.id}: {case.title}; step {index}: {step}") from error
