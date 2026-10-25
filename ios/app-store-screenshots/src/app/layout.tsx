import type { Metadata } from "next";
import "./globals.css";


export const metadata: Metadata = {
  title: "Pipedash App Store Screenshots",
  description: "Edit and export Pipedash screenshots for iPhone and iPad.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body style={{ fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif' }}>{children}</body>
    </html>
  );
}
