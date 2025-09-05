// Minimal placeholder for future TS integration of events.
// This file is not used by the Rust build.

export type Mode = 'InputStart' | 'InputDest' | 'InputDuration' | 'Timer';

export interface Place {
  id: string;
  name: string;
  embedded_type?: string | null;
}

export interface JourneyRow {
  date: string;   // YYYY-MM-DD
  dep: string;    // HH:MM
  arr: string;    // HH:MM
  durationMin: number;
  changes: number;
}

export interface AppConfig {
  start: { id: string; name: string };
  destination: { id: string; name: string };
  approach_minutes: number;
}

