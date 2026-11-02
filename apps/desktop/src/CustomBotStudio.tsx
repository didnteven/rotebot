import { useEffect, useRef, useState } from "react";
import type { Backend, BotDesign, ModelAccessStatus } from "./backend/types";
import { BotAvatar, BotStatus, removeCustomBotIdentity } from "./Bot";
import { safeBotImage, useBotLibrary } from "./BotLibrary";
import { Field, Notice, useAction } from "./ui";
const poseNames = ["Preview", "Working", "Waiting", "Happy", "Concerned", "Resting"];
const price = (images: number) => `US$${(images / 100).toFixed(2)}`;

export function CustomBotStudio({ backend, status, currentSeed, title = "", useBot, close, openSettings }: {
  backend: Backend; status: ModelAccessStatus; currentSeed: string; title?: string;
  useBot: (seed: string) => void; close: () => void; openSettings: () => void;
}) {
  const [description, setDescription] = useState(() => title.slice(0, 1000));
  const [design, setDesign] = useState<BotDesign | null>(null);
  const [saved, setSaved] = useState<BotDesign[]>([]);
  const [pendingDelete, setPendingDelete] = useState<BotDesign | null>(null);
  const [consent, setConsent] = useState(false);
  const [retryUncertain, setRetryUncertain] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const stopped = useRef(false);
  const previewHeading = useRef<HTMLHeadingElement>(null);
  const previewImage = safeBotImage(design?.images[0]);
  useEffect(() => {
    if (!previewImage) return;
    previewHeading.current?.focus({ preventScroll: true });
    previewHeading.current?.scrollIntoView?.({ block: "start", behavior: "instant" });
  }, [previewImage]);
  const { remember, forget } = useBotLibrary();
  const { act, busy, error, setError } = useAction();
  const blocked = !status.apiKey.present ? "Add a Meta API key in Settings to create a character." : status.egress !== "cloud_allowed" ? "Cloud work is paused. Turn off Local only in Privacy before generating." : null;
  useEffect(() => {
    const element = dialog.current;
    element?.showModal?.();
    void act(async () => {
      setSaved(await backend.listBotDesigns());
      if (currentSeed.startsWith("custom:")) {
        const next = await backend.getBotDesign(currentSeed.slice(7));
        setDesign(next); setDescription(next.description); remember(next);
      }
    });
    return () => { stopped.current = true; element?.close?.(); };
  }, [backend]);
  async function receive(next: BotDesign) {
    setDesign(next); remember(next);
    setSaved(current => [next, ...current.filter(bot => bot.id !== next.id)]);
  }
  async function deleteCharacter(bot: BotDesign) {
    await backend.deleteBotDesign(bot.id);
    forget(bot.id);
    removeCustomBotIdentity(bot.id);
    if (currentSeed === `custom:${bot.id}`) useBot(`draft-${crypto.randomUUID()}`);
    setSaved(current => current.filter(value => value.id !== bot.id));
    if (design?.id === bot.id) {
      setDesign(null); setConsent(false); setRetryUncertain(false);
    }
    setPendingDelete(null);
  }
  async function generate(preview: boolean) {
    stopped.current = false;
    const id = preview ? crypto.randomUUID() : design!.id;
    const text = preview ? description.trim() : design!.description;
    const missing = preview ? [0] : [1, 2, 3, 4, 5].filter(pose => !design!.completedPoses.includes(pose));
    try {
      for (const pose of missing) {
        if (stopped.current) break;
        setProgress(`Creating ${poseNames[pose].toLowerCase()}${preview ? "" : ` (${missing.indexOf(pose) + 1} of ${missing.length})`}…`);
        const next = await backend.generateBotImage({ id, description: text, pose, approved: consent, retryUncertain });
        await receive(next);
        if (!(next.transparentPoses ?? []).includes(pose)) break;
        setRetryUncertain(false);
      }
    } catch (error) {
      // Recover the durable partial set; never retry a paid request automatically.
      try { await receive(await backend.getBotDesign(id)); } catch { /* The request may have failed before creating a design. */ }
      throw error;
    } finally { setProgress(null); setConsent(false); }
  }
  const missing = design ? [1, 2, 3, 4, 5].filter(pose => !design.completedPoses.includes(pose)) : [];
  const complete = design?.completedPoses.length === 6;
  const transparentPreview = !!design?.transparentPoses?.includes(0);
  const needsCleanup = !!design && design.completedPoses.some(pose => !(design.transparentPoses ?? []).includes(pose));
  return <dialog ref={dialog} className="bot-studio" aria-labelledby="bot-studio-title" onCancel={event => { if (busy) event.preventDefault(); else close(); }}>
    <div className="bot-studio-header"><h2 id="bot-studio-title">Create a character</h2><button className="bot-studio-close ghost" disabled={busy} onClick={close} aria-label="Close character creator"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18" /></svg></button></div>
    <p className="muted bot-studio-intro">{design ? "A little personality for your everyday work." : "Imagine a companion. We’ll bring it to life."}</p>
    {(design || progress) && <section className="bot-preview" aria-labelledby="bot-preview-title" aria-busy={!!progress && !previewImage}>
      <h3 className="bot-studio-sr-only" id="bot-preview-title" ref={previewHeading} tabIndex={-1}>Your character preview</h3>
      <div className="bot-preview-stage">{previewImage ? <img className={`bot-preview-image${transparentPreview ? " is-transparent" : ""}`} src={previewImage} alt="Preview pose" /> : <div className="bot-preview-empty"><BotAvatar seed={currentSeed} state={progress ? "building" : "idle"} size={112} /><p>{progress ? "Creating your preview… It will appear here." : "Your preview will appear here."}</p></div>}</div>
      {previewImage && <>
        <p className="bot-preview-description">{design?.description}</p>
        <p className="bot-preview-saved muted">{transparentPreview ? "Saved to your characters" : "Background cleanup needed"}</p>
        <button className="primary" disabled={busy || !transparentPreview || needsCleanup} onClick={() => { useBot(`custom:${design!.id}`); close(); }}>{complete ? "Use this character" : "Use preview for now"}</button>
      </>}
    </section>}
    {design && needsCleanup && <div className="notice notice-warning"><p>{design.backgroundRemovalError ?? "This saved design still needs background removal."}</p><p>Your original images are kept. Cleanup runs on this Mac and makes no paid image-generation requests.</p><button disabled={busy} onClick={() => void act(async () => { setProgress("Removing backgrounds on this Mac…"); try { await receive(await backend.removeBotBackgrounds(design.id)); } finally { setProgress(null); } })}>Remove backgrounds · no API charge</button></div>}
    {blocked && <div className="notice notice-info"><p>{blocked}</p>{!status.apiKey.present && <button onClick={() => { close(); openSettings(); }}>Open Settings</button>}</div>}
    <Notice error={error} />
    {progress && <BotStatus seed={currentSeed} label={progress} detail="Includes local background removal. Each finished image is saved; closing the app may interrupt the current image." />}
    {busy && progress && !progress.startsWith("Removing backgrounds") && <button onClick={() => { stopped.current = true; }}>Stop after this image</button>}
    {!design ? <>
      <Field label="Describe your character"><textarea autoFocus rows={3} maxLength={1000} disabled={busy} value={description} onChange={event => setDescription(event.target.value)} placeholder="A charcoal fox, a minimal coffee bean, or a small slate robot" /></Field>
      <p className="muted bot-studio-price">Preview · about US$0.01 using your Meta API key.</p>
      <label className="choice"><input type="checkbox" checked={consent} disabled={busy} onChange={event => setConsent(event.target.checked)} />I approve the preview request and estimated US$0.01 charge.</label>
      <button className="primary" disabled={busy || !!blocked || !consent || !description.trim()} onClick={() => void act(() => generate(true))}>Generate preview · US$0.01</button>
    </> : <>
      <details className="bot-studio-disclosure" key={design.id}>
        <summary>{complete ? "View expressions" : "Add expressions"}<span className="muted">{complete ? "6 poses saved" : `Optional · about ${price(missing.length)}`}</span></summary>
        <p className="muted">{complete ? "Your full set of expressions is ready." : "Add working, waiting, happy, concerned, and resting poses. Your preview is the reference."}</p>
      {design.uncertainPose !== null && <div className="notice notice-warning"><p>The last image request may have been charged without returning a usable image.</p><label className="choice"><input type="checkbox" checked={retryUncertain} disabled={busy} onChange={event => setRetryUncertain(event.target.checked)} />I understand retrying may add another US$0.01 charge.</label></div>}
      {!complete && design.completedPoses.includes(0) && <>
        <label className="choice"><input type="checkbox" checked={consent} disabled={busy} onChange={event => setConsent(event.target.checked)} />I approve sending this preview to Meta and the estimated {price(missing.length)} charge.</label>
        <button className="primary" disabled={busy || !!blocked || !consent || !transparentPreview || needsCleanup || (design.uncertainPose !== null && !retryUncertain)} onClick={() => void act(() => generate(false))}>Generate {missing.length} poses · {price(missing.length)}</button>
      </>}
      {!design.completedPoses.includes(0) && <p>This preview could not be saved. Choose Start another design to try again; the previous request may have been charged.</p>}
      {design.completedPoses.length > 1 && <section aria-label="Your character expressions"><h3>Matching expressions · {design.completedPoses.filter(pose => pose !== 0).length} of 5 saved</h3><div className="bot-pose-gallery">{poseNames.slice(1).map((name, index) => { const pose = index + 1; return <figure key={name}>{safeBotImage(design.images[pose]) ? <img src={safeBotImage(design.images[pose])} alt={`${name} pose`} /> : <div className="bot-pose-placeholder"><span>Not created yet</span></div>}<figcaption>{name}</figcaption></figure>; })}</div></section>}
      </details>
      <div className="bot-studio-secondary"><button className="ghost" disabled={busy} onClick={() => { setDesign(null); setConsent(false); setRetryUncertain(false); setError(null); }}>Start another design</button><button className="ghost" disabled={busy} onClick={() => setPendingDelete(design)}>Delete character</button></div>
    </>}
    {!!saved.length && <details className="bot-studio-disclosure"><summary>Your saved characters<span className="muted">{saved.length}</span></summary><section aria-label="Your saved characters"><ul className="plain-list">{saved.map(bot => <li className="saved-character-row" key={bot.id}><button className="saved-character-open" disabled={busy} onClick={() => void act(async () => { await receive(await backend.getBotDesign(bot.id)); setDescription(bot.description); setConsent(false); setRetryUncertain(false); })}>{bot.description} · {bot.completedPoses.length}/6 poses</button><button className="danger" disabled={busy} aria-label={`Delete ${bot.description}`} onClick={() => setPendingDelete(bot)}>Delete</button></li>)}</ul></section></details>}
    {pendingDelete && <section className="notice notice-warning" role="alertdialog" aria-modal="false" aria-labelledby="delete-character-title" aria-describedby="delete-character-detail">
      <h3 id="delete-character-title">Delete “{pendingDelete.description}”?</h3>
      <p id="delete-character-detail">This permanently removes this character and all its saved images. Tasks using it will show a built-in bot. Your tasks are kept.</p>
      <div className="actions"><button className="danger" disabled={busy} onClick={() => void act(() => deleteCharacter(pendingDelete))}>Delete permanently</button><button autoFocus disabled={busy} onClick={() => setPendingDelete(null)}>Keep character</button></div>
    </section>}

    <details className="bot-studio-disclosure bot-studio-info">
      <summary>Generation details &amp; privacy</summary>
      <p className="muted">Uses your saved Meta API key. Your description and a built-in face reference are sent to Meta. Additional poses use your generated preview.</p>
      <p className="muted">A preview costs about US$0.01; all six poses cost about US$0.06. Prices can change and retries cost extra. Each paid request needs your approval.</p>
      {design && <>
        <p className="muted">{design.completedPoses.length} of 6 poses saved · Estimated request spend: {price(design.attempts)}.</p>
        {!needsCleanup && <button disabled={busy} onClick={() => void act(async () => { setProgress("Removing backgrounds on this Mac…"); try { await receive(await backend.removeBotBackgrounds(design.id)); } finally { setProgress(null); } })}>Repair cutouts · no API charge</button>}
      </>}
      <p className="muted">Images are saved and reused locally at no extra charge. Backgrounds are removed on this Mac; originals are kept for cleanup retries.</p>
    </details>
  </dialog>;
}
