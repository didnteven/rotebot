import { useEffect, useRef, useState } from "react";
import type { Backend, BotDesign, ModelAccessStatus } from "./backend/types";
import { BotAvatar, BotStatus } from "./Bot";
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
  const { remember } = useBotLibrary();
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
    <div className="bot-studio-header"><h2 id="bot-studio-title">Create a character</h2><button disabled={busy} onClick={close} aria-label="Close character creator">Close</button></div>
    <p>Create a robot, animal, object, or imaginary creature. We’ll use simple shapes, restrained colors, and a calm, confident look unless you describe a different style.</p>
    <section className="bot-preview" aria-labelledby="bot-preview-title" aria-busy={!!progress && !previewImage}>
      <h3 id="bot-preview-title" ref={previewHeading} tabIndex={-1}>Your character preview</h3>
      {previewImage ? <img className={`bot-preview-image${transparentPreview ? " is-transparent" : ""}`} src={previewImage} alt="Preview pose" /> : <div className="bot-preview-empty"><BotAvatar seed={currentSeed} state={progress ? "building" : "idle"} size={112} /><p>{progress ? "Creating your preview… It will appear here." : "Your preview will appear here."}</p></div>}
      {previewImage && <p className="muted">{transparentPreview ? "Transparent preview saved." : "Original saved. Remove its background before applying it."} Choose “{complete ? "Use this character" : "Use preview for now"}” below to show it on your task. You can reopen it later under Your saved characters.</p>}
    </section>
    <p className="muted">Uses your saved Meta API key, even if another AI service builds your tools. Your description and a built-in face reference are sent to Meta. Additional poses use your generated preview.</p>
    {design && needsCleanup && <div className="notice notice-warning"><p>{design.backgroundRemovalError ?? "This saved design still needs background removal."}</p><p>Your original images are kept. Cleanup runs on this Mac and makes no paid image-generation requests.</p><button disabled={busy} onClick={() => void act(async () => { setProgress("Removing backgrounds on this Mac…"); try { await receive(await backend.removeBotBackgrounds(design.id)); } finally { setProgress(null); } })}>Remove backgrounds · no API charge</button></div>}
    {blocked && <div className="notice notice-info"><p>{blocked}</p>{!status.apiKey.present && <button onClick={() => { close(); openSettings(); }}>Open Settings</button>}</div>}
    <Notice error={error} />
    {progress && <BotStatus seed={currentSeed} label={progress} detail="Includes local background removal. Each finished image is saved; closing the app may interrupt the current image." />}
    {busy && progress && !progress.startsWith("Removing backgrounds") && <button onClick={() => { stopped.current = true; }}>Stop after this image</button>}
    {!design ? <>
      <Field label="Describe your character"><textarea autoFocus rows={3} maxLength={1000} disabled={busy} value={description} onChange={event => setDescription(event.target.value)} placeholder="A charcoal fox, a minimal coffee bean, or a small slate robot" /></Field>
      <p className="muted">Choose its shape, color, or personality. New robots vary in shape and palette while sharing the oval-eye style.</p>
      <p>Preview: estimated US$0.01. A complete set of six poses is estimated at US$0.06. Prices can change; retries cost extra.</p>
      <label className="choice"><input type="checkbox" checked={consent} disabled={busy} onChange={event => setConsent(event.target.checked)} />I approve the preview request and estimated US$0.01 charge.</label>
      <button className="primary" disabled={busy || !!blocked || !consent || !description.trim()} onClick={() => void act(() => generate(true))}>Generate preview · US$0.01</button>
    </> : <>
      <p>{design.description}</p>

      <p>{design.completedPoses.length} of 6 poses saved · Request budget used: approximately {price(design.attempts)}. Images are reused locally with no generation charge when your tool runs. Background removal is processed on this Mac.</p>
      {design.uncertainPose !== null && <div className="notice notice-warning"><p>The last image request may have been charged without returning a usable image.</p><label className="choice"><input type="checkbox" checked={retryUncertain} disabled={busy} onChange={event => setRetryUncertain(event.target.checked)} />I understand retrying may add another US$0.01 charge.</label></div>}
      {!complete && design.completedPoses.includes(0) && <>
        <p>Next: approve the remaining {missing.length} poses. They use this image as their character reference.</p>
        <label className="choice"><input type="checkbox" checked={consent} disabled={busy} onChange={event => setConsent(event.target.checked)} />I approve sending this preview to Meta and the estimated {price(missing.length)} charge.</label>
        <button className="primary" disabled={busy || !!blocked || !consent || !transparentPreview || needsCleanup || (design.uncertainPose !== null && !retryUncertain)} onClick={() => void act(() => generate(false))}>Generate {missing.length} poses · {price(missing.length)}</button>
      </>}
      {!design.completedPoses.includes(0) && <p>This preview could not be saved. Choose Start another design to try again; the previous request may have been charged.</p>}
      <div className="actions"><button className={complete ? "primary" : undefined} disabled={busy || !transparentPreview || needsCleanup} onClick={() => { useBot(`custom:${design.id}`); close(); }}>{complete ? "Use this character" : "Use preview for now"}</button><button className="ghost" disabled={busy} onClick={() => { setDesign(null); setConsent(false); setRetryUncertain(false); setError(null); }}>Start another design</button></div>
      {design.completedPoses.length > 1 && <section aria-label="Your character expressions"><h3>Matching expressions · {design.completedPoses.filter(pose => pose !== 0).length} of 5 saved</h3><div className="bot-pose-gallery">{poseNames.slice(1).map((name, index) => { const pose = index + 1; return <figure key={name}>{safeBotImage(design.images[pose]) ? <img src={safeBotImage(design.images[pose])} alt={`${name} pose`} /> : <div className="bot-pose-placeholder"><span>Not created yet</span></div>}<figcaption>{name}</figcaption></figure>; })}</div></section>}
    </>}
    {!!saved.length && <details><summary>Your saved characters · continue or reuse</summary><ul className="plain-list">{saved.map(bot => <li key={bot.id}><button disabled={busy} onClick={() => void act(async () => { await receive(await backend.getBotDesign(bot.id)); setDescription(bot.description); setConsent(false); setRetryUncertain(false); })}>{bot.description} · {bot.completedPoses.length}/6 poses</button></li>)}</ul></details>}
    <p className="muted">Individual images avoid sprite-sheet cropping. Backgrounds are removed locally and the results are saved as transparent PNGs; originals are kept for free cleanup retries. The set includes six expressions with gentle app animations, rather than a generated video.</p>
  </dialog>;
}
