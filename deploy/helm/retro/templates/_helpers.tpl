{{- define "retro.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "retro.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else if contains (include "retro.name" .) .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name (include "retro.name" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "retro.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
app.kubernetes.io/name: {{ include "retro.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{/* Selector labels; call with (dict "root" $ "component" "backend"). */}}
{{- define "retro.selectorLabels" -}}
app.kubernetes.io/name: {{ include "retro.name" .root }}
app.kubernetes.io/instance: {{ .root.Release.Name }}
app.kubernetes.io/component: {{ .component }}
{{- end -}}

{{- define "retro.image" -}}
{{- printf "%s:%s" .image.repository (default .root.Chart.AppVersion .image.tag) -}}
{{- end -}}

{{- define "retro.host" -}}
{{- required "ingress.host is required" .Values.ingress.host -}}
{{- end -}}

{{- define "retro.publicUrl" -}}
{{- if .Values.publicUrl -}}
{{- .Values.publicUrl | trimSuffix "/" -}}
{{- else -}}
{{- printf "%s://%s" (ternary "https" "http" .Values.ingress.tls.enabled) (include "retro.host" .) -}}
{{- end -}}
{{- end -}}

{{- define "retro.devPostgres" -}}
{{- if and .Values.devMode.enabled .Values.postgres.enabled }}true{{ end -}}
{{- end -}}

{{- define "retro.devDex" -}}
{{- if and .Values.devMode.enabled .Values.dex.enabled }}true{{ end -}}
{{- end -}}

{{- define "retro.databaseHost" -}}
{{- if .Values.database.host -}}
{{- .Values.database.host -}}
{{- else if include "retro.devPostgres" . -}}
{{- printf "%s-postgres" (include "retro.fullname" .) -}}
{{- else -}}
{{- required "database.host is required" .Values.database.host -}}
{{- end -}}
{{- end -}}

{{- define "retro.oidcIssuerUrl" -}}
{{- if .Values.oidc.issuerUrl -}}
{{- .Values.oidc.issuerUrl -}}
{{- else if include "retro.devDex" . -}}
{{- printf "%s/dex" (include "retro.publicUrl" .) -}}
{{- else -}}
{{- required "oidc.issuerUrl is required when oidc.enabled" .Values.oidc.issuerUrl -}}
{{- end -}}
{{- end -}}

{{- define "retro.authSecretName" -}}
{{- default (printf "%s-auth" (include "retro.fullname" .)) .Values.auth.existingSecret -}}
{{- end -}}

{{- define "retro.databaseSecretName" -}}
{{- default (printf "%s-database" (include "retro.fullname" .)) .Values.database.existingSecret -}}
{{- end -}}

{{/* Hardening is relaxed in devMode only. */}}
{{- define "retro.podSecurityContext" -}}
{{- if not .Values.devMode.enabled }}
securityContext:
  {{- toYaml .Values.podSecurityContext | nindent 2 }}
{{- end }}
{{- end -}}

{{- define "retro.containerSecurityContext" -}}
{{- if not .Values.devMode.enabled }}
securityContext:
  {{- toYaml .Values.containerSecurityContext | nindent 2 }}
{{- end }}
{{- end -}}

{{/* Environment shared by the backend pods and the migration job. */}}
{{- define "retro.backendEnv" -}}
- name: DB_PASSWORD
  valueFrom:
    secretKeyRef:
      name: {{ include "retro.databaseSecretName" . }}
      key: DB_PASSWORD
- name: JWT_SECRET
  valueFrom:
    secretKeyRef:
      name: {{ include "retro.authSecretName" . }}
      key: JWT_SECRET
- name: STATE_SECRET
  valueFrom:
    secretKeyRef:
      name: {{ include "retro.authSecretName" . }}
      key: STATE_SECRET
{{- if .Values.oidc.enabled }}
- name: OIDC_CLIENT_SECRET
  valueFrom:
    secretKeyRef:
      name: {{ include "retro.authSecretName" . }}
      key: OIDC_CLIENT_SECRET
{{- end }}
{{- end -}}
