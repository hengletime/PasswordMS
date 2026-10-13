// 冲突三选一语义（PRD Q3）：默认不自动选择，三选项效果互斥且明确。

import { describe, expect, it } from 'vitest';

import {
  choiceEffect,
  CONFLICT_CHOICES,
  DEFAULT_CONFLICT_CHOICE,
} from './conflict';
import type { ConflictChoice } from '../types';

describe('conflict choice logic', () => {
  it('has no default selection (never auto-choose)', () => {
    expect(DEFAULT_CONFLICT_CHOICE).toBeNull();
  });

  it('exposes exactly the three裁定 options', () => {
    expect(CONFLICT_CHOICES).toEqual<ConflictChoice[]>(['keep_local', 'take_remote', 'keep_both']);
  });

  it('keep_local keeps the local version and drops the remote', () => {
    expect(choiceEffect('keep_local')).toEqual({
      keepLocal: true,
      takeRemote: false,
      duplicateRemote: false,
    });
  });

  it('take_remote overwrites local with remote', () => {
    expect(choiceEffect('take_remote')).toEqual({
      keepLocal: false,
      takeRemote: true,
      duplicateRemote: false,
    });
  });

  it('keep_both retains local and duplicates remote', () => {
    expect(choiceEffect('keep_both')).toEqual({
      keepLocal: true,
      takeRemote: false,
      duplicateRemote: true,
    });
  });

  it('effects are mutually consistent (a choice never both drops and takes)', () => {
    for (const choice of CONFLICT_CHOICES) {
      const effect = choiceEffect(choice);
      if (effect.takeRemote) {
        expect(effect.keepLocal).toBe(false);
        expect(effect.duplicateRemote).toBe(false);
      }
    }
  });
});
