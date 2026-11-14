<script setup lang="ts">
/**
 * `/invite/:token`: the invitee sees who invited them to which team and
 * accepts or declines (spec §6.1). Whoever holds the link may accept it.
 */
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute, useRouter } from 'vue-router';

import { api, errorCode, type InvitationPreviewDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import TeamTile from '@/components/TeamTile.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import LinkLayout from '@/features/invitations/LinkLayout.vue';
import LinkStateCard from '@/features/invitations/LinkStateCard.vue';
import { relativeFromNow } from '@/lib/time';
import { useTeamsStore } from '@/stores/teams';

type State =
  | { kind: 'loading' }
  | { kind: 'open'; invitation: InvitationPreviewDto }
  | { kind: 'closed'; invitation: InvitationPreviewDto; reason: 'expired' | 'revoked' | 'used' }
  | { kind: 'not-found' }
  | { kind: 'error'; code: string };

const { t, locale } = useI18n();
const route = useRoute();
const router = useRouter();
const teams = useTeamsStore();

const token = computed(() => String(route.params.token));
const state = ref<State>({ kind: 'loading' });
const busy = ref(false);
const actionError = ref<string | null>(null);

function closedReason(status: InvitationPreviewDto['status']): 'expired' | 'revoked' | 'used' {
  if (status === 'expired') return 'expired';
  if (status === 'revoked') return 'revoked';
  return 'used';
}

async function load(): Promise<void> {
  try {
    const res = await api.getInvitation(encodeURIComponent(token.value));
    if (res.status === 200) {
      const invitation = res.data;
      if (invitation.alreadyMember) {
        await teams.load().catch(() => undefined);
        await router.replace({ name: 'team-meetings', params: { teamId: invitation.team.id } });
        return;
      }
      state.value =
        invitation.status === 'pending'
          ? { kind: 'open', invitation }
          : { kind: 'closed', invitation, reason: closedReason(invitation.status) };
    } else if (res.status === 404) {
      state.value = { kind: 'not-found' };
    } else {
      state.value = { kind: 'error', code: res.status === 429 ? 'too_many_requests' : 'unknown' };
    }
  } catch {
    state.value = { kind: 'error', code: 'unknown' };
  }
}

onMounted(load);

async function accept(): Promise<void> {
  busy.value = true;
  actionError.value = null;
  try {
    const team = await teams.acceptInvitation(token.value);
    await router.push({ name: 'team-meetings', params: { teamId: team.id } });
  } catch (e) {
    actionError.value = errorCode(e);
    await load();
  } finally {
    busy.value = false;
  }
}

async function decline(): Promise<void> {
  busy.value = true;
  actionError.value = null;
  try {
    await teams.declineInvitation(token.value);
    await router.push({ name: 'home' });
  } catch (e) {
    actionError.value = errorCode(e);
    await load();
  } finally {
    busy.value = false;
  }
}

const closedText = computed(() => {
  if (state.value.kind !== 'closed') return '';
  const { invitation, reason } = state.value;
  const team = invitation.team.name;
  if (reason === 'expired') {
    return invitation.invitedBy
      ? t('linkStates.expiredText', { inviter: invitation.invitedBy.displayName, team })
      : t('linkStates.expiredTextNoInviter', { team });
  }
  if (reason === 'revoked') return t('linkStates.revokedText', { email: invitation.email });
  return t('linkStates.usedText', { team });
});
</script>

<template>
  <LinkLayout>
    <p v-if="state.kind === 'loading'" class="busy" aria-busy="true">{{ t('common.loading') }}</p>

    <section v-else-if="state.kind === 'open'" class="card" aria-labelledby="inv-title">
      <div class="card__head">
        <TeamTile :name="state.invitation.team.name" :size="56" />
        <div class="card__titles">
          <span class="card__kicker">{{ t('invitation.kicker') }}</span>
          <h1 id="inv-title" class="card__title">
            {{ t('invitation.title', { team: state.invitation.team.name }) }}
          </h1>
        </div>
      </div>

      <div class="inviter">
        <template v-if="state.invitation.invitedBy">
          <UserAvatar
            :user-id="state.invitation.invitedBy.id"
            :name="state.invitation.invitedBy.displayName"
            :size="36"
          />
          <i18n-t keypath="invitation.invitedBy" tag="p" class="inviter__text">
            <template #name>
              <strong class="inviter__name">{{ state.invitation.invitedBy.displayName }}</strong>
            </template>
          </i18n-t>
        </template>
        <p v-else class="inviter__text">{{ t('invitation.invited') }}</p>
      </div>

      <dl class="facts">
        <div class="facts__row">
          <dt>{{ t('invitation.members') }}</dt>
          <dd class="stack">
            <UserAvatar
              v-for="(member, i) in state.invitation.members"
              :key="member.id"
              :user-id="member.id"
              :name="member.displayName"
              :size="26"
              ring="var(--well)"
              :title="member.displayName"
              :style="{ marginLeft: i === 0 ? '0' : '-5px' }"
            />
            <span
              v-if="state.invitation.memberCount > state.invitation.members.length"
              class="stack__more"
            >
              +{{ state.invitation.memberCount - state.invitation.members.length }}
            </span>
          </dd>
        </div>
        <div class="facts__row">
          <dt>{{ t('invitation.sentTo') }}</dt>
          <dd class="facts__value">{{ state.invitation.email }}</dd>
        </div>
        <div class="facts__row">
          <dt>{{ t('invitation.expires') }}</dt>
          <dd class="facts__value">{{ relativeFromNow(state.invitation.expiresAt, locale) }}</dd>
        </div>
      </dl>

      <AppAlert v-if="actionError">{{ t(`errors.${actionError}`) }}</AppAlert>

      <div class="actions">
        <AppButton size="page" class="actions__decline" :disabled="busy" @click="decline">
          {{ t('invitation.decline') }}
        </AppButton>
        <AppButton
          variant="primary"
          size="page"
          class="actions__accept"
          :disabled="busy"
          @click="accept"
        >
          <AppIcon name="check" :size="17" />
          {{ t('invitation.accept') }}
        </AppButton>
      </div>
    </section>

    <LinkStateCard
      v-else-if="state.kind === 'closed'"
      :icon="state.reason === 'expired' ? 'clock' : 'close'"
      :title="t(`linkStates.${state.reason}Title`)"
      :text="closedText"
    >
      <AppButton to="/">{{ t('common.backToRetro') }}</AppButton>
    </LinkStateCard>

    <LinkStateCard
      v-else
      icon="alert"
      :title="t('linkStates.notFoundTitle')"
      :text="state.kind === 'error' ? t(`errors.${state.code}`) : t('linkStates.notFoundText')"
    >
      <AppButton to="/">{{ t('common.backToRetro') }}</AppButton>
    </LinkStateCard>
  </LinkLayout>
</template>

<style scoped>
.busy {
  color: var(--muted);
}
.card {
  display: flex;
  flex-direction: column;
  gap: 22px;
  width: 100%;
  max-width: 480px;
  padding: 36px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.card__head {
  display: flex;
  align-items: center;
  gap: 14px;
}
.card__titles {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.card__kicker {
  font-size: 13px;
  font-weight: 600;
  color: var(--muted);
}
.card__title {
  margin: 0;
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.01em;
  overflow-wrap: anywhere;
}
.inviter {
  display: flex;
  align-items: center;
  gap: 12px;
}
.inviter__text {
  margin: 0;
  font-size: 15px;
  color: var(--muted);
}
.inviter__name {
  color: var(--text);
}
.facts {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 0;
  padding: 14px 16px;
  border-radius: var(--radius-control);
  background: var(--well);
}
.facts__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  font-size: 13px;
}
.facts__row dt {
  color: var(--muted);
}
.facts__row dd {
  margin: 0;
}
.facts__value {
  font-weight: 600;
  overflow-wrap: anywhere;
  text-align: right;
}
.stack {
  display: flex;
  align-items: center;
}
.stack__more {
  margin-left: 6px;
  font-weight: 600;
  color: var(--muted);
}
.actions {
  display: flex;
  gap: 10px;
}
.actions__decline {
  flex: 1 1 0;
}
.actions__accept {
  flex: 2 1 0;
}
@media (max-width: 560px) {
  .card {
    padding: 24px;
  }
}
</style>
