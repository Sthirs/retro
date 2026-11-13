<script setup lang="ts">
/**
 * Onboarding (spec §4): a signed-in user with no team creates one or accepts
 * a pending invitation.
 */
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';

import { errorCode } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppTopBar from '@/components/layout/AppTopBar.vue';
import TeamTile from '@/components/TeamTile.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import TextField from '@/components/ui/TextField.vue';
import { relativeFromNow } from '@/lib/time';
import { useSessionStore } from '@/stores/session';
import { useTeamsStore } from '@/stores/teams';

const { t, locale } = useI18n();
const router = useRouter();
const session = useSessionStore();
const teams = useTeamsStore();

const name = ref('');
const nameError = ref<string | null>(null);
const creating = ref(false);
const busyToken = ref<string | null>(null);
const inviteError = ref<string | null>(null);
const loadError = ref(false);

const firstName = computed(() => session.user?.displayName.trim().split(/\s+/)[0] ?? '');

onMounted(async () => {
  try {
    await teams.ensureLoaded();
  } catch {
    loadError.value = true;
  }
});

async function retry(): Promise<void> {
  loadError.value = false;
  try {
    await teams.load();
  } catch {
    loadError.value = true;
  }
}

async function create(): Promise<void> {
  if (creating.value) return;
  if (!name.value.trim()) {
    nameError.value = 'invalid_team_name';
    return;
  }
  creating.value = true;
  nameError.value = null;
  try {
    const team = await teams.createTeam(name.value);
    await router.push({ name: 'team-meetings', params: { teamId: team.id } });
  } catch (e) {
    nameError.value = errorCode(e);
  } finally {
    creating.value = false;
  }
}

async function accept(token: string): Promise<void> {
  busyToken.value = token;
  inviteError.value = null;
  try {
    const team = await teams.acceptInvitation(token);
    await router.push({ name: 'team-meetings', params: { teamId: team.id } });
  } catch (e) {
    inviteError.value = errorCode(e);
    await teams.load().catch(() => undefined);
  } finally {
    busyToken.value = null;
  }
}

async function decline(token: string): Promise<void> {
  busyToken.value = token;
  inviteError.value = null;
  try {
    await teams.declineInvitation(token);
  } catch (e) {
    inviteError.value = errorCode(e);
    await teams.load().catch(() => undefined);
  } finally {
    busyToken.value = null;
  }
}

function expires(iso: string): string {
  return relativeFromNow(iso, locale.value);
}
</script>

<template>
  <div v-if="session.user" class="page">
    <AppTopBar />
    <main class="main">
      <div class="content">
        <div class="heading">
          <h1 class="heading__title">{{ t('onboarding.title', { name: firstName }) }}</h1>
          <p class="heading__subtitle">{{ t('onboarding.subtitle') }}</p>
        </div>

        <AppAlert v-if="loadError">
          {{ t('errors.unknown') }}
          <button type="button" class="link-button" @click="retry">{{ t('common.retry') }}</button>
        </AppAlert>

        <div class="grid">
          <section class="card card--create" aria-labelledby="ct">
            <span class="card__icon" aria-hidden="true"><AppIcon name="users" :size="22" /></span>
            <div class="card__heading">
              <h2 id="ct" class="card__title">{{ t('onboarding.createTitle') }}</h2>
              <p class="card__text">{{ t('onboarding.createText') }}</p>
            </div>
            <form class="create" novalidate @submit.prevent="create">
              <TextField
                id="team-name"
                v-model="name"
                dense
                maxlength="60"
                autocomplete="off"
                :label="t('createTeam.name')"
                :placeholder="t('createTeam.placeholder')"
                :error="nameError ? t(`errors.${nameError}`) : undefined"
              />
              <AppButton type="submit" variant="primary" size="touch" :disabled="creating">
                {{ creating ? t('createTeam.creating') : t('createTeam.submit') }}
              </AppButton>
            </form>
          </section>

          <section class="card" aria-labelledby="pi">
            <div class="card__row">
              <h2 id="pi" class="card__title">{{ t('onboarding.invitationsTitle') }}</h2>
              <span v-if="teams.invitations.length" class="count">{{
                teams.invitations.length
              }}</span>
            </div>
            <template v-if="teams.invitations.length">
              <p class="card__text">{{ t('onboarding.sentTo', { email: session.user.email }) }}</p>
              <ul class="invites">
                <li v-for="invite in teams.invitations" :key="invite.token" class="invite">
                  <TeamTile :name="invite.team.name" :size="40" />
                  <div class="invite__text">
                    <span class="invite__team">{{ invite.team.name }}</span>
                    <span class="invite__meta">
                      {{
                        invite.invitedBy
                          ? t('onboarding.invitedBy', {
                              name: invite.invitedBy.displayName,
                              when: expires(invite.expiresAt),
                            })
                          : t('onboarding.expires', { when: expires(invite.expiresAt) })
                      }}
                    </span>
                  </div>
                  <AppButton
                    variant="ghost"
                    :disabled="busyToken === invite.token"
                    :aria-label="t('userMenu.decline', { team: invite.team.name })"
                    @click="decline(invite.token)"
                  >
                    {{ t('onboarding.decline') }}
                  </AppButton>
                  <AppButton
                    variant="primary"
                    :disabled="busyToken === invite.token"
                    :aria-label="t('userMenu.accept', { team: invite.team.name })"
                    @click="accept(invite.token)"
                  >
                    {{ t('onboarding.accept') }}
                  </AppButton>
                </li>
              </ul>
            </template>
            <p v-else class="card__text">
              {{ t('onboarding.none', { email: session.user.email }) }}
            </p>
            <AppAlert v-if="inviteError">{{ t(`errors.${inviteError}`) }}</AppAlert>
            <p class="card__hint">{{ t('onboarding.linkHint') }}</p>
          </section>
        </div>
      </div>
    </main>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}
.main {
  flex: 1 1 auto;
  padding: 28px 32px;
}
.content {
  display: flex;
  flex-direction: column;
  gap: 24px;
  width: 100%;
  max-width: 960px;
  margin: 0 auto;
  padding-top: 24px;
}
.heading {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.heading__title {
  margin: 0;
  font-size: 32px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.heading__subtitle {
  margin: 0;
  font-size: 15px;
  color: var(--muted);
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(320px, 100%), 1fr));
  align-items: start;
  gap: 20px;
}
.card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 24px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.card--create {
  gap: 16px;
}
.card__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border-radius: var(--radius-control);
  background: var(--accent-soft);
  color: var(--accent-text);
}
.card__heading {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.card__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.card__title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.card__text {
  margin: 0;
  color: var(--muted);
}
.card__hint {
  margin: 0;
  font-size: 13px;
  color: var(--muted);
}
.create {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 16px;
}
.create > :first-child {
  align-self: stretch;
}
.count {
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  background: var(--accent);
  color: var(--on-accent);
  font-size: 12px;
  font-weight: 700;
}
.invites {
  margin: 0;
  padding: 0;
  list-style: none;
}
.invite {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  padding: 12px 0;
  border-top: 1px solid var(--line);
}
.invite__text {
  display: flex;
  flex: 1 1 160px;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.invite__team {
  font-weight: 700;
}
.invite__meta {
  font-size: 13px;
  color: var(--muted);
}
.link-button {
  margin-left: 8px;
  padding: 0;
  border: none;
  background: none;
  color: inherit;
  font: inherit;
  text-decoration: underline;
  cursor: pointer;
}
@media (max-width: 560px) {
  .main {
    padding: 20px 16px;
  }
  .content {
    padding-top: 0;
  }
  .heading__title {
    font-size: 26px;
  }
}
</style>
