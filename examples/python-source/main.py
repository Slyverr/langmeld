def greet(name: str, greeting: str = "Hello") -> str:
    return format_greeting(name, greeting)


def format_greeting(name: str, greeting: str) -> str:
    return f"{greeting}, {name}!"
