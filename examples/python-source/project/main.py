from .greetings import greet as greet_user, greet_alternate


def main(name: str) -> str:
    return greet_user(name) + " / " + greet_alternate(name)
