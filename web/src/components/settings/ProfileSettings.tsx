import React, { useCallback, useEffect, useMemo, useState } from "react";
import type { UserProfile } from "@/types";
import { profileApi } from "@/api/client";
import { Btn, FieldInput, FieldLabel, FieldTextarea } from "./shared";

// ─── Section card ─────────────────────────────────────────────────────────────

function SectionCard({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        background: "var(--bg-tertiary)",
        border: "1px solid var(--border-subtle)",
        borderRadius: 10,
        padding: "16px 18px",
        marginBottom: 16,
      }}
    >
      <div
        style={{
          marginBottom: subtitle ? 4 : 14,
          fontSize: 13,
          fontWeight: 600,
          color: "var(--text-primary)",
        }}
      >
        {title}
      </div>
      {subtitle && (
        <div
          style={{
            fontSize: 12,
            color: "var(--text-tertiary)",
            marginBottom: 14,
            lineHeight: 1.5,
          }}
        >
          {subtitle}
        </div>
      )}
      {children}
    </div>
  );
}

// ─── ProfileField ─────────────────────────────────────────────────────────────

function ProfileField({
  label,
  field,
  value,
  placeholder,
  saving,
  onBlurSave,
}: {
  label: string;
  field: keyof UserProfile;
  value: string;
  placeholder?: string;
  saving: boolean;
  onBlurSave: (field: keyof UserProfile, value: string) => void;
}) {
  const [localValue, setLocalValue] = useState(value);

  useEffect(() => {
    setLocalValue(value);
  }, [value]);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
      <FieldLabel>
        {label}
        {saving && (
          <span
            style={{
              marginLeft: 6,
              fontSize: 10,
              color: "var(--text-tertiary)",
            }}
          >
            Saving…
          </span>
        )}
      </FieldLabel>
      <FieldInput
        value={localValue}
        placeholder={placeholder}
        disabled={saving}
        onChange={(e) => setLocalValue(e.target.value)}
        onBlur={() => onBlurSave(field, localValue)}
        style={{ opacity: saving ? 0.6 : 1 }}
      />
    </div>
  );
}

// ─── AboutField ───────────────────────────────────────────────────────────────

const ABOUT_MAX = 500;

function AboutField({
  value,
  saving,
  onBlurSave,
}: {
  value: string;
  saving: boolean;
  onBlurSave: (field: keyof UserProfile, value: string) => void;
}) {
  const [localValue, setLocalValue] = useState(value);

  useEffect(() => {
    setLocalValue(value);
  }, [value]);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
      <FieldLabel>
        About
        {saving && (
          <span
            style={{
              marginLeft: 6,
              fontSize: 10,
              color: "var(--text-tertiary)",
            }}
          >
            Saving…
          </span>
        )}
      </FieldLabel>
      <FieldTextarea
        value={localValue}
        placeholder="A short bio shared with your personas as background context…"
        disabled={saving}
        onChange={(e) => setLocalValue(e.target.value.slice(0, ABOUT_MAX))}
        onBlur={() => onBlurSave("about", localValue)}
        style={{ minHeight: 80, opacity: saving ? 0.6 : 1 }}
      />
      <div
        style={{
          display: "flex",
          justifyContent: "flex-end",
          alignItems: "center",
        }}
      >
        <span
          style={{
            fontSize: 11,
            color:
              localValue.length > ABOUT_MAX * 0.9
                ? "var(--warning)"
                : "var(--text-tertiary)",
            flexShrink: 0,
            marginLeft: 8,
          }}
        >
          {localValue.length} / {ABOUT_MAX}
        </span>
      </div>
    </div>
  );
}

// ─── TimezoneField ────────────────────────────────────────────────────────────

function TimezoneField({
  value,
  detectedTimezone,
  saving,
  onBlurSave,
}: {
  value: string;
  detectedTimezone: string;
  saving: boolean;
  onBlurSave: (field: keyof UserProfile, value: string) => void;
}) {
  const [localValue, setLocalValue] = useState(value);

  useEffect(() => {
    setLocalValue(value);
  }, [value]);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <FieldLabel>
          Timezone
          {saving && (
            <span
              style={{
                marginLeft: 6,
                fontSize: 10,
                color: "var(--text-tertiary)",
              }}
            >
              Saving…
            </span>
          )}
        </FieldLabel>
        {detectedTimezone && value !== detectedTimezone && (
          <button
            type="button"
            onClick={() => onBlurSave("timezone", detectedTimezone)}
            style={{
              background: "none",
              border: "none",
              padding: "2px 0",
              fontSize: 11,
              color: "var(--accent-primary)",
              cursor: "pointer",
              fontFamily: "inherit",
            }}
          >
            Use detected: {detectedTimezone}
          </button>
        )}
      </div>
      <FieldInput
        value={localValue}
        placeholder={detectedTimezone || "e.g. America/Los_Angeles"}
        disabled={saving}
        onChange={(e) => setLocalValue(e.target.value)}
        onBlur={() => onBlurSave("timezone", localValue)}
        style={{ opacity: saving ? 0.6 : 1 }}
      />
    </div>
  );
}

// ─── ProfileSettings ──────────────────────────────────────────────────────────

export function ProfileSettings() {
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [profileSaving, setProfileSaving] = useState<string | null>(null);

  const detectedTimezone = useMemo(() => {
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      return "";
    }
  }, []);

  const loadProfile = useCallback(async () => {
    try {
      const res = await profileApi.get();
      setProfile(res.data);
    } catch {
      // non-critical
    }
  }, []);

  useEffect(() => {
    loadProfile();
  }, [loadProfile]);

  const handleProfileBlur = async (field: keyof UserProfile, value: string) => {
    const finalValue = value.trim() || null;
    try {
      setProfileSaving(field);
      const res = await profileApi.update({ [field]: finalValue });
      setProfile(res.data);
    } catch {
      // ignore — field will revert on next load
    } finally {
      setProfileSaving(null);
    }
  };

  return (
    <div>
      <div style={{ marginBottom: 20 }}>
        <div
          style={{
            fontSize: 16,
            fontWeight: 700,
            color: "var(--text-primary)",
            marginBottom: 4,
          }}
        >
          Profile
        </div>
        <div style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
          Personal context shared with all your personas.
        </div>
      </div>

      <SectionCard
        title="Your Info"
        subtitle="Used by your personas as background context when generating responses."
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <ProfileField
            label="Name"
            field="display_name"
            value={profile?.display_name ?? ""}
            saving={profileSaving === "display_name"}
            onBlurSave={handleProfileBlur}
          />
          <ProfileField
            label="Pronouns"
            field="pronouns"
            value={profile?.pronouns ?? ""}
            placeholder="e.g. she/her"
            saving={profileSaving === "pronouns"}
            onBlurSave={handleProfileBlur}
          />
          <ProfileField
            label="Role"
            field="role"
            value={profile?.role ?? ""}
            placeholder="e.g. Senior Software Engineer"
            saving={profileSaving === "role"}
            onBlurSave={handleProfileBlur}
          />
          <ProfileField
            label="Organization"
            field="organization"
            value={profile?.organization ?? ""}
            placeholder="e.g. Acme Corp"
            saving={profileSaving === "organization"}
            onBlurSave={handleProfileBlur}
          />
          <ProfileField
            label="Location"
            field="location"
            value={profile?.location ?? ""}
            placeholder="e.g. San Francisco, CA"
            saving={profileSaving === "location"}
            onBlurSave={handleProfileBlur}
          />
          <TimezoneField
            value={profile?.timezone ?? ""}
            detectedTimezone={detectedTimezone}
            saving={profileSaving === "timezone"}
            onBlurSave={handleProfileBlur}
          />
          <AboutField
            value={profile?.about ?? ""}
            saving={profileSaving === "about"}
            onBlurSave={handleProfileBlur}
          />
        </div>
      </SectionCard>

      <div style={{ marginTop: 8 }}>
        <Btn variant="ghost" sm onClick={loadProfile}>
          Refresh
        </Btn>
      </div>
    </div>
  );
}
