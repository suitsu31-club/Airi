import type {Metadata} from "next";
import "./globals.css";
import { Providers } from "./providers";

export const metadata: Metadata = {
  title: "Airi",
  description: "Club membership manage system",
};

export default function RootLayout({children}: LayoutProps<"/">) {
  return (
      <html
        lang="en"
        suppressHydrationWarning
      >
      <body className="bg-background text-foreground">
        <Providers>
          {children}
        </Providers>
      </body>
      </html>
  );
}
