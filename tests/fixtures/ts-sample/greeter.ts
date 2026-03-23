export interface Greeter {
  greet(name: string): string;
}

export class DefaultGreeter implements Greeter {
  private prefix: string;

  constructor(prefix: string) {
    this.prefix = prefix;
  }

  greet(name: string): string {
    return `${this.prefix}, ${name}!`;
  }
}

export enum Mood {
  Happy,
  Sad,
  Neutral,
}

export type MoodMap = Record<string, Mood>;

export const DEFAULT_MOOD: Mood = Mood.Neutral;

export function createGreeter(prefix: string): Greeter {
  return new DefaultGreeter(prefix);
}

// Re-export with alias
export { DefaultGreeter as Greeter2 };
