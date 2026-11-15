<script setup lang="ts">
/** Phase 1 (Icebreaker artboard): the greeting, the question, who is answering. */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import PresenceAvatar from '@/components/PresenceAvatar.vue';
import { useMeetingStore } from '@/stores/meeting';

const { t } = useI18n();
const store = useMeetingStore();
const meeting = computed(() => store.meeting!);
</script>

<template>
  <section class="ice" :aria-label="t('phases.icebreaker')">
    <div class="ice__greeting">
      <span class="ice__hello" :lang="meeting.greeting.langCode">{{ meeting.greeting.text }}</span>
      <span class="ice__say">
        <span v-if="meeting.greeting.pronunciation" class="ice__pronunciation">
          {{ meeting.greeting.pronunciation }}
        </span>
        <span class="ice__language">{{ meeting.greeting.language }}</span>
      </span>
    </div>
    <div class="ice__question">
      <span class="label">{{ t('icebreaker.question') }}</span>
      <p class="ice__question-text">{{ meeting.question }}</p>
    </div>
    <div v-if="store.currentSpeaker" class="speaker" aria-live="polite">
      <PresenceAvatar
        :user-id="store.currentSpeaker.id"
        :name="store.currentSpeaker.displayName"
        :picture-version="store.currentSpeaker.pictureVersion"
        :online="store.currentSpeaker.online"
        :size="96"
        ring="var(--well)"
      />
      <div class="speaker__text">
        <span class="speaker__meta">
          {{
            t('icebreaker.answering', { n: store.speakerIndex + 1, total: store.speakers.length })
          }}
        </span>
        <span class="speaker__name">{{ store.currentSpeaker.displayName }}</span>
        <span class="speaker__meta">
          {{
            store.nextSpeaker
              ? t('icebreaker.upNext', { name: store.nextSpeaker.displayName })
              : t('icebreaker.lastOne')
          }}
        </span>
      </div>
    </div>
    <p class="ice__note">{{ t('icebreaker.footnote') }}</p>
  </section>
</template>

<style scoped>
.ice {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: 28px;
  padding: 32px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.ice__greeting {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 20px;
}
.ice__hello {
  color: var(--accent-text);
  font-size: 64px;
  font-weight: 800;
  letter-spacing: -0.03em;
  line-height: 1.1;
}
.ice__say {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.ice__pronunciation {
  font-size: 18px;
}
.ice__language {
  color: var(--muted);
  font-size: 14px;
}
.ice__question {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.label {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.ice__question-text {
  margin: 0;
  font-size: 26px;
  font-weight: 700;
  line-height: 1.3;
}
.speaker {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 20px;
  padding: 24px;
  border-radius: var(--radius-panel);
  background: var(--well);
}
.speaker__text {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.speaker__meta {
  color: var(--muted);
  font-size: 13px;
}
.speaker__name {
  font-size: 30px;
  font-weight: 700;
}
.ice__note {
  margin: 0;
  color: var(--muted);
  font-size: 13px;
  line-height: 1.5;
}
@media (max-width: 600px) {
  .ice {
    padding: 20px;
  }
  .ice__hello {
    font-size: 44px;
  }
}
</style>
