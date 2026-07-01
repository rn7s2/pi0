import type { Pi0Api } from './shared/ipc';

declare global {
  interface Window {
    pi0: Pi0Api;
  }
}

export {};
