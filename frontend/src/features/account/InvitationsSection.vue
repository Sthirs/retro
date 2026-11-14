<script setup lang="ts">
/** "Pending invitations": the invitations sent to the user's email, with Accept and Decline. */
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import TeamTile from '@/components/TeamTile.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { usePendingInvitations } from '@/composables/usePendingInvitations';
import AccountPanel from '@/features/account/AccountPanel.vue';
import { relativeFromNow } from '@/lib/time';
import { useSessionStore } from '@/stores/session';

const { t, locale } = useI18n();
const session = useSessionStore();
const { invitations, busy, error, accept, decline } = usePendingInvitations();
</script>

<template>
  <AccountPanel
    v-if="invitations.length"
    id="invitations"
    :title="t('account.invitations.title', { count: invitations.length })"
    :description="t('account.invitations.sentTo', { email: session.user?.email ?? '' })"
  >
    <AppAlert v-if="error" class="error">{{ t(`errors.${error}`) }}</AppAlert>
    <ul class="list">
      <li v-for="invite in invitations" :key="invite.token" class="row" data-testid="invite-row">
        <TeamTile :name="invite.team.name" :size="40" />
        <div class="row__text">
          <span class="row__name">{{ invite.team.name }}</span>
          <span class="row__meta">
            {{
              invite.invitedBy
                ? t('account.invitations.invitedBy', {
                    name: invite.invitedBy.displayName,
                    when: relativeFromNow(invite.expiresAt, locale),
                  })
                : t('account.invitations.expires', {
                    when: relativeFromNow(invite.expiresAt, locale),
                  })
            }}
          </span>
        </div>
        <div class="row__actions">
          <AppButton
            variant="ghost"
            size="touch"
            :disabled="busy === invite.token"
            :aria-label="t('account.invitations.declineLabel', { team: invite.team.name })"
            @click="decline(invite.token)"
          >
            {{ t('account.invitations.decline') }}
          </AppButton>
          <AppButton
            variant="primary"
            size="touch"
            :disabled="busy === invite.token"
            :aria-label="t('account.invitations.acceptLabel', { team: invite.team.name })"
            @click="accept(invite.token)"
          >
            <AppIcon name="check" :size="16" :stroke-width="2.5" />
            {{ t('account.invitations.accept') }}
          </AppButton>
        </div>
      </li>
    </ul>
  </AccountPanel>
</template>

<style scoped>
.error {
  margin: 0 20px 14px;
  width: auto;
}
.list {
  margin: 0;
  padding: 0;
  list-style: none;
}
.row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 14px;
  padding: 12px 20px;
  border-top: 1px solid var(--line);
}
.row__text {
  display: flex;
  flex: 1 1 200px;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.row__name {
  font-size: 15px;
  font-weight: 700;
}
.row__meta {
  font-size: 13px;
  color: var(--muted);
}
.row__actions {
  display: flex;
  gap: 8px;
}
</style>
