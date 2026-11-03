function formatGreeting(name: string, prefix = "Hello"): string {
  return `${prefix}, ${name}!`;
}

export async function main(name: string): Promise<string> {
  return formatGreeting(name, "Welcome");
}
