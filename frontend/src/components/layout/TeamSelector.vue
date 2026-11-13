<script setup lang="ts">
/**
 * Team selector (spec §7.1): lists the user's teams; picking one re-routes to
 * the same section of that team. "Add team" opens the create dialog.
 */
import { computed, nextTick, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute, useRouter } from 'vue-router';

import AppIcon from '@/components/AppIcon.vue';
import CreateTeamDialog from '@/components/layout/CreateTeamDialog.vue';
import TeamTile from '@/components/TeamTile.vue';
import { usePopover } from '@/composables/usePopover';
import { useTeamsStore } from '@/stores/teams';

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const teams = useTeamsStore();

const root = ref<HTMLElement | null>(null);
const trigger = ref<HTMLButtonElement | null>(null);
const menu = ref<HTMLElement | null>(null);
const { open, toggle, close } = usePopover(root, trigger);
const creating = ref(false);

const current = computed(() => teams.byId(String(route.params.teamId ?? '')));

function items(): HTMLElement[] {
  return Array.from(menu.value?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
}

async function openMenu(focus: 'first' | 'last' = 'first'): Promise<void> {
  open.value = true;
  await nextTick();
  const list = items();
  (focus === 'first' ? list[0] : list[list.length - 1])?.focus();
}

function onTriggerKey(event: KeyboardEvent): void {
  if (event.key === 'ArrowDown') {
    event.preventDefault();
    void openMenu('first');
  } else if (event.key === 'ArrowUp') {
    event.preventDefault();
    void openMenu('last');
  }
}

function onMenuKey(event: KeyboardEvent): void {
  const list = items();
  const index = list.indexOf(document.activeElement as HTMLElement);
  const moves: Record<string, number> = {
    ArrowDown: (index + 1) % list.length,
    ArrowUp: (index - 1 + list.length) % list.length,
    Home: 0,
    End: list.length - 1,
  };
  const next = moves[event.key];
  if (next !== undefined) {
    event.preventDefault();
    list[next]?.focus();
  } else if (event.key === 'Tab') {
    close();
  }
}

async function select(teamId: string): Promise<void> {
  close(true);
  if (teamId === current.value?.id) return;
  const name =
    typeof route.name === 'string' && route.name.startsWith('team-') ? route.name : 'team-meetings';
  await router.push({ name, params: { teamId } });
}

function addTeam(): void {
  close();
  creating.value = true;
}

function onToggle(): void {
  if (open.value) toggle();
  else void openMenu();
}
</script>

<template>
  <div ref="root" class="selector">
    <button
      ref="trigger"
      type="button"
      class="selector__trigger"
      aria-haspopup="menu"
      :aria-expanded="open"
      aria-controls="team-menu"
      data-testid="team-selector"
      @click="onToggle"
      @keydown="onTriggerKey"
    >
      <TeamTile v-if="current" :name="current.name" :size="28" />
      <span class="selector__text">
        <span class="selector__label">{{ t('topBar.team') }}</span>
        <span class="selector__name">{{ current?.name }}</span>
      </span>
      <AppIcon name="chevronDown" :size="16" />
    </button>
    <div
      v-show="open"
      id="team-menu"
      ref="menu"
      role="menu"
      class="menu"
      :aria-label="t('topBar.teams')"
      @keydown="onMenuKey"
    >
      <button
        v-for="team in teams.teams"
        :key="team.id"
        type="button"
        role="menuitem"
        class="menu__item"
        :class="{ 'menu__item--current': team.id === current?.id }"
        :aria-current="team.id === current?.id ? 'true' : undefined"
        tabindex="-1"
        @click="select(team.id)"
      >
        <TeamTile :name="team.name" :size="28" />
        <span class="menu__name">{{ team.name }}</span>
        <AppIcon
          v-if="team.id === current?.id"
          name="check"
          :size="16"
          :stroke-width="2.5"
          class="menu__check"
        />
      </button>
      <div role="separator" class="menu__separator" />
      <button
        type="button"
        role="menuitem"
        class="menu__item menu__item--add"
        tabindex="-1"
        @click="addTeam"
      >
        <span class="menu__add-icon"><AppIcon name="plus" /></span>
        <span>{{ t('topBar.addTeam') }}</span>
      </button>
    </div>
    <CreateTeamDialog :open="creating" @close="creating = false" />
  </div>
</template>

<style scoped>
.selector {
  position: relative;
}
.selector__trigger {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-height: 48px;
  max-width: 280px;
  padding: 0 12px 0 8px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}
.selector__trigger:hover {
  background: var(--well);
}
.selector__text {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  min-width: 0;
  line-height: 1.15;
}
.selector__label {
  font-size: 13px;
  color: var(--muted);
}
.selector__name {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.menu {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 20;
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 260px;
  max-width: calc(100vw - 32px);
  padding: 6px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-menu);
}
.menu__item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-height: 44px;
  padding: 0 12px;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 14px;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
}
.menu__item:hover,
.menu__item:focus-visible {
  background: var(--well);
}
.menu__item--current,
.menu__item--current:hover {
  background: var(--accent-soft);
}
.menu__name {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.menu__check {
  color: var(--accent-text);
}
.menu__separator {
  height: 1px;
  margin: 4px 6px;
  background: var(--line);
}
.menu__item--add {
  color: var(--accent-text);
  font-weight: 600;
}
.menu__add-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
}
</style>
