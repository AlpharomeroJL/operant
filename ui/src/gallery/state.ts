// The template gallery (docs/specs/ui.md's zero-code spirit applied to
// docs/specs/registry.md: browse cards, plain-language grants, one-click
// install). Turns the registry catalog (./catalog.ts) into browsable cards
// plus a one-workflow-at-a-time install preview. Pure and DOM-free, same
// split as ui/src/library/state.ts.
//
// Before a person can install anything, the preview shows the workflow's
// embedded plain-English step summary (manifest.step_summary, already
// plain sentences -- nothing to render) and its permissions via the real
// grant prompt (ui/src/grants/state.ts, U4A's renderer underneath): the
// same reused component, not a second copy of grant prose. Approval is a
// real Allow/Deny decision; nothing installs unexplained.

import type { BusClient } from "../bus/mockClient.ts";
import { createGrantPrompt, type GrantPrompt, type GrantPromptSnapshot } from "../grants/state.ts";
import { galleryStrings } from "./strings.ts";
import {
  loadFixturePublisherKeys,
  loadFixtureTemplates,
  verifyAndPin,
  PinStore,
  type TemplateManifest,
  type Trust,
} from "./catalog.ts";

export interface GalleryCard {
  name: string;
  title: string;
  summary: string;
  publisher: string;
  installLabel: string;
  installed: boolean;
}

export interface InstallPreview {
  name: string;
  title: string;
  publisher: string;
  stepLines: string[];
  stepsHeading: string;
  permissionsHeading: string;
  trustNote: string;
  grant: GrantPromptSnapshot;
}

export interface InstallError {
  title: string;
  why: string;
  action: string;
}

export interface GallerySnapshot {
  title: string;
  cards: GalleryCard[];
  empty: boolean;
  emptyLabel: string;
  preview?: InstallPreview;
  error?: InstallError;
  notice?: string;
}

export interface CreateGalleryOptions {
  templates?: readonly TemplateManifest[];
  publisherKeys?: Record<string, string>;
  pins?: PinStore;
}

export interface Gallery {
  getSnapshot(): GallerySnapshot;
  subscribe(fn: (snap: GallerySnapshot) => void): () => void;
  /** Opens the plain-language preview and permission prompt for `name`. No-op for an unknown name. */
  install(name: string): void;
  /** Approves the workflow currently in preview, if any. No-op otherwise. */
  allow(): void;
  /** Dismisses the workflow currently in preview, if any. No-op otherwise. */
  deny(): void;
  dispose(): void;
}

function trustNoteFor(publisher: string, trust: Trust): string {
  switch (trust) {
    case "trusted":
      return galleryStrings.trustedNote(publisher);
    case "first_time":
      return galleryStrings.firstTimeNote(publisher);
    case "unverified":
      return galleryStrings.unverifiedNote;
  }
}

export function createGallery(bus: BusClient, opts: CreateGalleryOptions = {}): Gallery {
  const templates = opts.templates ?? loadFixtureTemplates();
  const publisherKeys = opts.publisherKeys ?? loadFixturePublisherKeys();
  const pins = opts.pins ?? new PinStore();
  const installedNames = new Set<string>();
  const listeners = new Set<(snap: GallerySnapshot) => void>();

  let pending: { manifest: TemplateManifest; trust: Trust; grant: GrantPrompt } | undefined;
  let error: InstallError | undefined;
  let notice: string | undefined;

  function cardFor(t: TemplateManifest): GalleryCard {
    const installed = installedNames.has(t.name);
    return {
      name: t.name,
      title: t.description || t.name,
      summary: t.description,
      publisher: t.publisher,
      installLabel: installed ? galleryStrings.installed : galleryStrings.install,
      installed,
    };
  }

  function snapshot(): GallerySnapshot {
    const cards = templates.map(cardFor);
    const snap: GallerySnapshot = {
      title: galleryStrings.title,
      cards,
      empty: cards.length === 0,
      emptyLabel: galleryStrings.empty,
    };
    if (pending) {
      snap.preview = {
        name: pending.manifest.name,
        title: pending.manifest.description || pending.manifest.name,
        publisher: pending.manifest.publisher,
        stepLines: pending.manifest.step_summary,
        stepsHeading: galleryStrings.stepsHeading,
        permissionsHeading: galleryStrings.permissionsHeading,
        trustNote: trustNoteFor(pending.manifest.publisher, pending.trust),
        grant: pending.grant.getSnapshot(),
      };
    }
    if (error) snap.error = error;
    if (notice) snap.notice = notice;
    return snap;
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  function finish(manifest: TemplateManifest, trust: Trust): void {
    installedNames.add(manifest.name);
    pending = undefined;
    notice = galleryStrings.installedNotice(manifest.description || manifest.name);
    bus.publish("workflow.installed", {
      name: manifest.name,
      version: manifest.version,
      publisher: manifest.publisher,
      signed: trust !== "unverified",
      dry_run_only: trust !== "trusted",
    });
    emit();
  }

  function install(name: string): void {
    const manifest = templates.find((t) => t.name === name);
    if (!manifest) return;
    error = undefined;
    notice = undefined;

    let trust: Trust;
    try {
      trust = verifyAndPin(manifest, publisherKeys[manifest.publisher], pins);
    } catch {
      error = {
        title: galleryStrings.errorTitle,
        why: galleryStrings.errorWhy,
        action: galleryStrings.errorAction,
      };
      emit();
      return;
    }

    const grant = createGrantPrompt(manifest.capabilities, {
      onAllow: () => finish(manifest, trust),
      onDeny: () => {
        pending = undefined;
        notice = galleryStrings.cancelled;
        emit();
      },
    });
    pending = { manifest, trust, grant };
    emit();
  }

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    install,
    allow() {
      pending?.grant.allow();
    },
    deny() {
      pending?.grant.deny();
    },
    dispose() {
      listeners.clear();
    },
  };
}
