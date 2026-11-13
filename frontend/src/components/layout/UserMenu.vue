<script setup lang="ts">
/**
 * User menu (spec §7.1): identity, pending invitations, theme, log out. The
 * bell button in the top bar opens the same panel.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';

import { errorCode } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import type { IconName } from '@/components/icons';
import AppAlert from '@/components/ui/AppAlert.vue';
import TeamTile from '@/components/TeamTile.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import { usePopover } from '@/composables/usePopover';
import { applyThemePreference, readThemePreference, type ThemePreference } from '@/lib/theme';
import { useSessionStore } from '@/stores/session';
import { useTeamsStore } from '@/stores/teams';

const { t } = useI18n();
const router = useRouter();
const session = useSessionStore();
const teams = useTeamsStore();

const root = ref<HTMLElement | null>(null);
const trigger = ref<HTMLButtonElement | null>(null);
const { open, close } = usePopover(root, trigger);
const theme = ref<ThemePreference>(readThemePreference());
const busy = ref<string | null>(null);
const error = ref<string | null>(null);

const count = computed(() => teams.invitations.length);

const themes: { value: ThemePreference; icon: IconName }[] = [
  { value: 'light', icon: 'sun' },
  { value: 'dark', icon: 'moon' },
  { value: 'system', icon: 'monitor' },
];

function setTheme(value: ThemePreference): void {
  theme.value = value;
  applyThemePreference(value);
}

function toggle(): void {
  open.value = !open.value;
  error.value = null;
}

async function accept(token: string): Promise<void> {
  busy.value = token;
  error.value = null;
  try {
    const team = await teams.acceptInvitation(token);
    close();
    await router.push({ name: 'team-meetings', params: { teamId: team.id } });
  } catch (e) {
    error.value = errorCode(e);
    await teams.load().catch(() => undefined);
  } finally {
    busy.value = null;
  }
}

async function decline(token: string): Promise<void> {
  busy.value = token;
  error.value = null;
  try {
    await teams.declineInvitation(token);
  } catch (e) {
    error.value = errorCode(e);
    await teams.load().catch(() => undefined);
  } finally {
    busy.value = null;
  }
}

async function logout(): Promise<void> {
  close();
  await session.logout();
  await router.replace({ name: 'login' });
}

defineExpose({ toggle });
</script>

<template>
  <div v-if="session.user" ref="root" class="user">
    <button
      type="button"
      class="bell"
      :aria-label="count ? t('topBar.invitationsNew', { count }) : t('topBar.invitations')"
      :aria-expanded="open"
      aria-controls="user-menu"
      @click="toggle"
    >
      <AppIcon name="bell" :size="20" />
      <span v-if="count" class="bell__badge" aria-hidden="true">{{ count }}</span>
    </button>
    <button
      ref="trigger"
      type="button"
      class="trigger"
      :aria-label="t('topBar.userMenu')"
      :aria-expanded="open"
      aria-controls="user-menu"
      @click="toggle"
    >
      <UserAvatar :user-id="session.user.id" :name="session.user.displayName" />
      <span class="trigger__name" data-testid="current-user">{{ session.user.displayName }}</span>
      <AppIcon name="chevronDown" :size="16" />
    </button>

    <div
      v-show="open"
      id="user-menu"
      class="panel"
      role="region"
      :aria-label="t('topBar.userMenu')"
    >
      <div class="panel__identity">
        <UserAvatar :user-id="session.user.id" :name="session.user.displayName" :size="40" />
        <div class="panel__who">
          <span class="panel__name">{{ session.user.displayName }}</span>
          <span class="panel__email">{{ session.user.email }}</span>
        </div>
      </div>

      <template v-if="count">
        <div class="panel__separator" />
        <h2 class="panel__heading">{{ t('userMenu.pending', { count }) }}</h2>
        <ul class="invites">
          <li v-for="invite in teams.invitations" :key="invite.token" class="invite">
            <TeamTile :name="invite.team.name" :size="32" />
            <div class="invite__text">
              <span class="invite__team">{{ invite.team.name }}</span>
              <span v-if="invite.invitedBy" class="invite__from">
                {{ t('userMenu.from', { name: invite.invitedBy.displayName }) }}
              </span>
            </div>
            <button
              type="button"
              class="icon-btn"
              :aria-label="t('userMenu.decline', { team: invite.team.name })"
              :disabled="busy === invite.token"
              @click="decline(invite.token)"
            >
              <AppIcon name="close" />
            </button>
            <button
              type="button"
              class="icon-btn icon-btn--accept"
              :aria-label="t('userMenu.accept', { team: invite.team.name })"
              :disabled="busy === invite.token"
              @click="accept(invite.token)"
            >
              <AppIcon name="check" :size="16" :stroke-width="2.5" />
            </button>
          </li>
        </ul>
        <div v-if="error" class="panel__error">
          <AppAlert>{{ t(`errors.${error}`) }}</AppAlert>
        </div>
      </template>

      <div class="panel__separator" />
      <div class="panel__theme">
        <span id="theme-label" class="panel__heading">{{ t('userMenu.theme') }}</span>
        <div class="segmented" role="group" aria-labelledby="theme-label">
          <button
            v-for="option in themes"
            :key="option.value"
            type="button"
            class="segmented__option"
            :aria-pressed="theme === option.value"
            @click="setTheme(option.value)"
          >
            <AppIcon :name="option.icon" :size="15" />
            {{ t(`userMenu.${option.value}`) }}
          </button>
        </div>
      </div>

      <div class="panel__separator" />
      <button type="button" class="panel__item" @click="logout">
        <AppIcon name="logout" />
        {{ t('userMenu.logout') }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.user {
  position: relative;
  display: flex;
  align-items: center;
  gap: 4px;
}
.bell {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--text);
  cursor: pointer;
}
.bell:hover,
.trigger:hover {
  background: var(--well);
}
.bell__badge {
  position: absolute;
  top: 8px;
  right: 8px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  border-radius: var(--radius-pill);
  background: var(--accent);
  color: var(--on-accent);
  font-size: 10px;
  font-weight: 700;
  line-height: 16px;
  text-align: center;
}
.trigger {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 44px;
  padding: 0 6px;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}
@media (max-width: 560px) {
  .trigger__name {
    display: none;
  }
}
.panel {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  z-index: 20;
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 320px;
  max-width: calc(100vw - 32px);
  padding: 6px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-menu);
}
.panel__identity {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
}
.panel__who {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.panel__name {
  font-weight: 700;
}
.panel__email {
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 13px;
  color: var(--muted);
}
.panel__separator {
  height: 1px;
  margin: 4px 6px;
  background: var(--line);
}
.panel__heading {
  margin: 0;
  padding: 8px 12px 2px;
  font-size: 13px;
  font-weight: 600;
  color: var(--muted);
}
.invites {
  margin: 0;
  padding: 0;
  list-style: none;
}
.invite {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
}
.invite__text {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  min-width: 0;
}
.invite__team {
  font-size: 14px;
  font-weight: 600;
}
.invite__from {
  font-size: 12px;
  color: var(--muted);
}
.icon-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 32px;
  height: 32px;
  padding: 0;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.icon-btn::before {
  /* 44 px touch target. */
  content: '';
  position: absolute;
  inset: -6px;
}
.icon-btn:hover {
  background: var(--well);
  color: var(--text);
}
.icon-btn--accept,
.icon-btn--accept:hover {
  background: var(--accent);
  color: var(--on-accent);
}
.icon-btn:disabled {
  cursor: progress;
  opacity: 0.75;
}
.panel__error {
  padding: 4px 12px 8px;
}
.panel__theme {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 0 12px 8px;
}
.panel__theme .panel__heading {
  padding: 6px 0 0;
}
.segmented {
  display: flex;
  gap: 4px;
  padding: 3px;
  border-radius: var(--radius-control);
  background: var(--well);
}
.segmented__option {
  display: inline-flex;
  flex: 1 1 0;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 34px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--muted);
  font-family: var(--font-sans);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.segmented__option[aria-pressed='true'] {
  background: var(--surface);
  color: var(--text);
  box-shadow: var(--shadow-card);
}
.panel__item {
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
.panel__item:hover {
  background: var(--well);
}
</style>
