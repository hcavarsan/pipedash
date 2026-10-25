"use client";
import * as React from "react";
import { ImagePlus } from "lucide-react";
import { createImageMask } from "@/components/editor/create-image-mask";
import { img } from "@/lib/image-cache";
import type { ImageElement } from "@/lib/types";

type Props = {
  element: ImageElement;
  editable?: boolean;
};

// Image overlay content. Placement, rotation, and drag/resize handles come from
// the shared Movable wrapper in slide-canvas so overlays behave like text.
export function ImageElementCanvas({ element, editable }: Props) {
  const source = img(element.src);
  const maskImage = createImageMask(element.fade);

  if (!source) {
    // The empty placeholder is an editing affordance only; never export it.
    if (!editable) return null;
    return (
      <div className="flex h-full w-full items-center justify-center border border-dashed border-current/40 bg-black/10 text-current/70">
        <ImagePlus className="h-6 w-6" aria-hidden />
        <span className="sr-only">Pick an image in the inspector</span>
      </div>
    );
  }

  return (
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={source}
      alt=""
      draggable={false}
      style={{
        width: "100%",
        height: "100%",
        display: "block",
        objectFit: element.fit || "cover",
        maskImage,
        WebkitMaskImage: maskImage,
      }}
    />
  );
}
