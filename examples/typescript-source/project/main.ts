import { formatGreeting as greet } from "./greetings";
import alternate from "./other.ts";

export function main(): string {
  return `${greet("Ada", "Welcome")} ${alternate("Grace", "Hello")}`;
}
