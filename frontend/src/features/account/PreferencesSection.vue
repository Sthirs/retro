<script setup lang="ts">
/** "Preferences": theme, language and the team Retro opens by default (spec §7.2). */
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import type { IconName } from '@/components/icons';
import AccountPanel from '@/features/account/AccountPanel.vue';
import { useThemePreference, type ThemePreference } from '@/lib/theme';
import { useTeamsStore } from '@/stores/teams';

const { t } = useI18n();
const teams = useTeamsStore();

const { theme, setTheme } = useThemePreference();

const themes: { value: ThemePreference; icon: IconName }[] = [
  { value: 'light', icon: 'sun' },
  { value: 'dark', icon: 'moon' },
  { value: 'system', icon: 'monitor' },
];

function setDefaultTeam(event: Event): void {
  const id = (event.target as HTMLSelectElement).value;
  teams.remember(id);
}
</script>

<template>
  <AccountPanel id="preferences" :title="t('account.preferences.title')">
    <div class="prefs">
      <div class="pref">
        <div class="pref__text">
          <span id="theme-label" class="pref__label">{{ t('userMenu.theme') }}</span>
          <span class="pref__hint">{{ t('account.preferences.themeHint') }}</span>
        </div>
        <div class="segmented" role="group" aria-labelledby="theme-label">
          <button
            v-for="option in themes"
            :key="option.value"
            type="button"
            class="segmented__option"
            :aria-pressed="theme === option.value"
            @click="setTheme(option.value)"
          >
            <AppIcon :name="option.icon" :size="16" />
            {{ t(`userMenu.${option.value}`) }}
          </button>
        </div>
      </div>

      <div class="pref">
        <div class="pref__text">
          <label for="language" class="pref__label">{{ t('account.preferences.language') }}</label>
          <span class="pref__hint">{{ t('account.preferences.languageHint') }}</span>
        </div>
        <div class="select">
          <select id="language" class="select__input">
            <option value="en">{{ t('account.preferences.english') }}</option>
          </select>
          <AppIcon class="select__chevron" name="chevronDown" :size="16" />
        </div>
      </div>

      <div v-if="teams.teams.length" class="pref">
        <div class="pref__text">
          <label for="default-team" class="pref__label">
            {{ t('account.preferences.defaultTeam') }}
          </label>
          <span class="pref__hint">{{ t('account.preferences.defaultTeamHint') }}</span>
        </div>
        <div class="select">
          <select
            id="default-team"
            class="select__input"
            :value="teams.defaultTeam()?.id ?? ''"
            @change="setDefaultTeam"
          >
            <option v-for="team in teams.teams" :key="team.id" :value="team.id">
              {{ team.name }}
            </option>
          </select>
          <AppIcon class="select__chevron" name="chevronDown" :size="16" />
        </div>
      </div>
    </div>
  </AccountPanel>
</template>

<style scoped>
.prefs {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 0 20px 20px;
}
.pref {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.pref__text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.pref__label {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}
.pref__hint {
  font-size: 13px;
  color: var(--muted);
}
.segmented {
  display: flex;
  gap: 4px;
  box-sizing: border-box;
  width: 340px;
  max-width: 100%;
  padding: 4px;
  border-radius: var(--radius-control);
  background: var(--well);
}
.segmented__option {
  display: inline-flex;
  flex: 1 1 0;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 44px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--muted);
  font-family: var(--font-sans);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}
.segmented__option[aria-pressed='true'] {
  background: var(--surface);
  color: var(--text);
  box-shadow: var(--shadow-card);
}
.select {
  position: relative;
  width: 340px;
  max-width: 100%;
}
.select__input {
  box-sizing: border-box;
  width: 100%;
  height: 44px;
  padding: 0 40px 0 14px;
  appearance: none;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}
.select__chevron {
  position: absolute;
  top: 14px;
  right: 14px;
  color: var(--muted);
  pointer-events: none;
}
</style>
