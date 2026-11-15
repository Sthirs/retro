<script setup lang="ts">
/** "Preferences": theme, language and the team Retro opens by default (spec §4, §7.1). */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import TeamTile from '@/components/TeamTile.vue';
import type { IconName } from '@/components/icons';
import SelectMenu from '@/components/ui/SelectMenu.vue';
import type { SelectOption } from '@/components/ui/select';
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

const languages = computed<SelectOption[]>(() => [
  { value: 'en', label: t('account.preferences.english') },
]);

// '' is "Last used": no pinned team.
const teamOptions = computed<SelectOption[]>(() => [
  {
    value: '',
    label: t('account.preferences.lastUsed'),
    hint: t('account.preferences.lastUsedHint'),
  },
  ...teams.teams.map((team) => ({
    value: team.id,
    label: team.name,
    hint: t(
      'account.preferences.teamHint',
      { role: t(`common.${team.role}`), count: team.memberCount },
      team.memberCount,
    ),
  })),
]);

function setDefaultTeam(value: string | string[]): void {
  const id = Array.isArray(value) ? value[0] : value;
  teams.pin(id || null);
}
</script>

<template>
  <AccountPanel id="preferences" :title="t('account.preferences.title')" overflow-visible>
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
          <span id="language-label" class="pref__label">
            {{ t('account.preferences.language') }}
          </span>
          <span class="pref__hint">{{ t('account.preferences.languageHint') }}</span>
        </div>
        <SelectMenu
          class="field"
          model-value="en"
          :options="languages"
          :label="t('account.preferences.language')"
          :width="340"
        >
          <template #trigger="{ attrs, open }">
            <button
              type="button"
              class="field__trigger"
              :class="{ 'field__trigger--open': open }"
              aria-labelledby="language-label language-value"
              v-bind="attrs"
            >
              <span id="language-value" class="field__value">
                {{ t('account.preferences.english') }}
              </span>
              <AppIcon
                name="chevronDown"
                :size="16"
                class="field__chevron"
                :class="{ 'field__chevron--open': open }"
              />
            </button>
          </template>
        </SelectMenu>
      </div>

      <div v-if="teams.teams.length" class="pref">
        <div class="pref__text">
          <span id="default-team-label" class="pref__label">
            {{ t('account.preferences.defaultTeam') }}
          </span>
          <span class="pref__hint">{{ t('account.preferences.defaultTeamHint') }}</span>
        </div>
        <SelectMenu
          class="field"
          :model-value="teams.pinnedDefault?.id ?? ''"
          :options="teamOptions"
          :label="t('account.preferences.defaultTeam')"
          :pinned="1"
          :width="340"
          data-testid="default-team"
          @update:model-value="setDefaultTeam"
        >
          <template #trigger="{ attrs, open }">
            <button
              type="button"
              class="field__trigger"
              :class="{ 'field__trigger--open': open }"
              aria-labelledby="default-team-label default-team-value"
              v-bind="attrs"
            >
              <span id="default-team-value" class="field__value">
                <TeamTile v-if="teams.pinnedDefault" :name="teams.pinnedDefault.name" :size="24" />
                <span v-else class="icon-tile icon-tile--small">
                  <AppIcon name="clock" :size="14" />
                </span>
                <span class="field__text">
                  {{ teams.pinnedDefault?.name ?? t('account.preferences.lastUsed') }}
                </span>
              </span>
              <AppIcon
                name="chevronDown"
                :size="16"
                class="field__chevron"
                :class="{ 'field__chevron--open': open }"
              />
            </button>
          </template>
          <template #leading="{ option }">
            <TeamTile v-if="option.value" :name="option.label" :size="28" />
            <span v-else class="icon-tile"><AppIcon name="clock" :size="15" /></span>
          </template>
        </SelectMenu>
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
  min-width: 0;
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
.field {
  width: 340px;
}
.field__trigger {
  display: flex;
  align-items: center;
  gap: 10px;
  box-sizing: border-box;
  width: 100%;
  min-height: 44px;
  padding: 0 12px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 14px;
  font-weight: 600;
  text-align: left;
  cursor: pointer;
}
.field__trigger--open {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}
.field__trigger:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.field__value {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 10px;
}
.field__text {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.field__chevron {
  color: var(--muted);
  transition: transform 0.15s ease;
}
.field__chevron--open {
  transform: rotate(180deg);
}
.icon-tile {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm);
  background: var(--well);
  color: var(--muted);
}
.icon-tile--small {
  width: 24px;
  height: 24px;
  border-radius: 7px;
}
@media (prefers-reduced-motion: reduce) {
  .field__chevron {
    transition: none;
  }
}
</style>
