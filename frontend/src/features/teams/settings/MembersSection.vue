<script setup lang="ts">
/**
 * Members with roles (spec §5.1, §5.2). Admins promote, demote and remove;
 * anyone can leave. The last admin is protected by the server.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';

import { api, apiError, errorCode, type MemberDto, type TeamDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import SettingsPanel from '@/features/teams/settings/SettingsPanel.vue';
import { useTeamsStore } from '@/stores/teams';

const props = defineProps<{
  team: TeamDto;
  members: MemberDto[];
  isAdmin: boolean;
  currentUserId: string;
}>();
const emit = defineEmits<{ changed: [] }>();
const { t } = useI18n();
const router = useRouter();
const teams = useTeamsStore();

const error = ref<string | null>(null);
const busyId = ref<string | null>(null);
const confirming = ref<{ kind: 'leave' | 'remove'; member: MemberDto } | null>(null);
const confirmError = ref<string | null>(null);

/** You first, then admins, then by name (the server sends admins first). */
const ordered = computed(() =>
  [...props.members].sort(
    (a, b) => Number(b.user.id === props.currentUserId) - Number(a.user.id === props.currentUserId),
  ),
);
const soleMember = computed(() => props.members.length === 1);

async function setRole(member: MemberDto, role: 'admin' | 'member'): Promise<void> {
  busyId.value = member.user.id;
  error.value = null;
  try {
    const res = await api.updateMember(props.team.id, member.user.id, { role });
    if (res.status !== 204) throw apiError(res);
  } catch (e) {
    error.value = errorCode(e);
  } finally {
    busyId.value = null;
    emit('changed');
  }
}

function ask(kind: 'leave' | 'remove', member: MemberDto): void {
  confirmError.value = null;
  confirming.value = { kind, member };
}

async function confirm(): Promise<void> {
  const pending = confirming.value;
  if (!pending) return;
  busyId.value = pending.member.user.id;
  confirmError.value = null;
  try {
    const res = await api.removeMember(props.team.id, pending.member.user.id);
    if (res.status !== 204) throw apiError(res);
    confirming.value = null;
    if (pending.kind === 'leave') {
      await teams.load();
      await router.push({ name: 'home' });
      return;
    }
    emit('changed');
  } catch (e) {
    confirmError.value = t(`errors.${errorCode(e)}`);
  } finally {
    busyId.value = null;
  }
}

const dialog = computed(() => {
  const pending = confirming.value;
  if (!pending) return null;
  if (pending.kind === 'leave') {
    return soleMember.value
      ? {
          title: t('settings.leave.lastTitle', { team: props.team.name }),
          text: t('settings.leave.lastText'),
          confirm: t('settings.leave.lastConfirm'),
        }
      : {
          title: t('settings.leave.title', { team: props.team.name }),
          text: t('settings.leave.text'),
          confirm: t('settings.leave.confirm'),
        };
  }
  const name = pending.member.user.displayName;
  return {
    title: t('settings.remove.title', { name }),
    text: t('settings.remove.text', { name, team: props.team.name }),
    confirm: t('settings.remove.confirm'),
  };
});
</script>

<template>
  <SettingsPanel
    id="ms"
    :title="t('settings.members.title', { count: members.length })"
    :description="t('settings.members.hint')"
  >
    <AppAlert v-if="error" class="error">{{ t(`errors.${error}`) }}</AppAlert>
    <ul class="list">
      <li v-for="member in ordered" :key="member.user.id" class="item" data-testid="member">
        <UserAvatar :user-id="member.user.id" :name="member.user.displayName" :size="36" />
        <div class="item__text">
          <span class="item__name">
            {{
              member.user.id === currentUserId
                ? t('settings.members.you', { name: member.user.displayName })
                : member.user.displayName
            }}
          </span>
          <span class="item__email">{{ member.user.email }}</span>
        </div>
        <span class="tag" :class="member.role === 'admin' ? 'tag--admin' : 'tag--member'">
          {{ member.role === 'admin' ? t('common.admin') : t('common.member') }}
        </span>
        <div class="item__actions">
          <AppButton
            v-if="member.user.id === currentUserId"
            variant="ghost"
            size="compact"
            @click="ask('leave', member)"
          >
            <AppIcon name="logout" :size="15" />
            {{ t('settings.members.leave') }}
          </AppButton>
          <template v-else-if="isAdmin">
            <AppButton
              variant="ghost"
              size="compact"
              :disabled="busyId === member.user.id"
              @click="setRole(member, member.role === 'admin' ? 'member' : 'admin')"
            >
              {{
                member.role === 'admin'
                  ? t('settings.members.makeMember')
                  : t('settings.members.makeAdmin')
              }}
            </AppButton>
            <button
              type="button"
              class="icon-btn"
              :aria-label="t('settings.members.remove', { name: member.user.displayName })"
              :disabled="busyId === member.user.id"
              @click="ask('remove', member)"
            >
              <AppIcon name="trash" />
            </button>
          </template>
        </div>
      </li>
    </ul>
    <ConfirmDialog
      :open="!!confirming"
      :title="dialog?.title ?? ''"
      :text="dialog?.text ?? ''"
      :confirm-label="dialog?.confirm ?? ''"
      :busy="busyId !== null"
      :error="confirmError"
      danger
      @close="confirming = null"
      @confirm="confirm"
    />
  </SettingsPanel>
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
.item {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 14px;
  padding: 10px 20px;
  border-top: 1px solid var(--line);
}
.item__text {
  display: flex;
  flex: 1 1 200px;
  flex-direction: column;
  min-width: 0;
}
.item__name {
  font-size: 14px;
  font-weight: 700;
}
.item__email {
  overflow-wrap: anywhere;
  font-size: 13px;
  color: var(--muted);
}
.tag {
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}
.tag--admin {
  background: var(--accent-soft);
  color: var(--accent-text);
}
.tag--member {
  background: var(--well);
  color: var(--muted);
}
.item__actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
  min-width: 170px;
}
.icon-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 36px;
  height: 36px;
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
  inset: -4px;
}
.icon-btn:hover:not(:disabled) {
  background: var(--well);
  color: var(--danger);
}
.icon-btn:disabled {
  cursor: progress;
  opacity: 0.75;
}
@media (max-width: 560px) {
  .item__actions {
    min-width: 0;
  }
}
</style>
