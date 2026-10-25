// Headline formulas surfaced in the inspector's "Copy ideas" menu. Picking one
// drops the formula into the headline for the active locale; the bracketed
// words are placeholders the user replaces. The fuller library (category
// banks, deck arcs, localization notes) lives in the skill's copy-ideas.md.

export type CopyIdea = {
  formula: string;  // inserted into the headline; "\n" is an intentional break
  example: string;  // shown under the formula so the pattern is obvious
};

export type CopyIdeaSlot = {
  id: string;
  name: string;
  label: string;    // suggested eyebrow label for this slot
  ideas: CopyIdea[];
};

export const COPY_IDEA_SLOTS: CopyIdeaSlot[] = [
  {
    id: "hero",
    name: "Hero",
    label: "MEET [APP]",
    ideas: [
      { formula: "[Outcome],\nwithout [pain].", example: "Great coffee, without the guesswork." },
      { formula: "Your [noun],\n[benefit].", example: "Your money, finally clear." },
      { formula: "[Verb] [noun].\n[Verb] [noun].", example: "Track habits. Keep streaks." },
      { formula: "Every [noun],\n[outcome].", example: "Every workout, counted." },
      { formula: "Meet your new\n[role].", example: "Meet your new sleep coach." },
    ],
  },
  {
    id: "differentiator",
    name: "Differentiator",
    label: "ONLY ON [APP]",
    ideas: [
      { formula: "Only [app]\n[does the unique thing].", example: "Only Bloom knows when beans peak." },
      { formula: "No [pain].\nJust [outcome].", example: "No ads. Just progress." },
      { formula: "Built for\n[specific person].", example: "Built for people who read slowly." },
      { formula: "[Verb] it once.\n[Outcome] forever.", example: "Scan it once. Tracked forever." },
    ],
  },
  {
    id: "feature",
    name: "Feature",
    label: "FEATURE 01",
    ideas: [
      { formula: "[Verb] [noun]\nin [time].", example: "Log a meal in 3 seconds." },
      { formula: "Your [noun],\nsorted.", example: "Your shelf, sorted by freshness." },
      { formula: "Never [pain]\nagain.", example: "Never miss a refill again." },
      { formula: "From [before]\nto [after].", example: "From receipt to budget in one tap." },
      { formula: "Right on your\n[surface].", example: "Right on your lock screen." },
    ],
  },
  {
    id: "proof",
    name: "Proof",
    label: "LOVED BY [GROUP]",
    ideas: [
      { formula: "[Number] [people]\n[verb] [app].", example: "40,000 runners train with Stride." },
      { formula: "Loved by\n[specific group].", example: "Loved by home baristas." },
      { formula: "Private\nby design.", example: "Your data stays on device." },
    ],
  },
  {
    id: "closer",
    name: "Closer",
    label: "AND MORE",
    ideas: [
      { formula: "And so\nmuch more.", example: "Pair with feature pills or a wordlist." },
      { formula: "Made for people\nwho [care about X].", example: "Made for people who care about the details." },
      { formula: "Your first [unit]\nstarts today.", example: "Your first week starts today." },
    ],
  },
];
