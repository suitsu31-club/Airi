import { Zen_Kurenaido } from "next/font/google";
import Image from "next/image";
import { ThemeSwitcher } from "@/components/theme/theme-switcher";
import cat from "@/public/kazusa-cat.svg";

const zenKurenaido = Zen_Kurenaido({ weight: "400" });

export default function AuthLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <main className="container mx-auto mt-64 p-2">
      <div className="flex flex-row items-center gap-4 mb-8">
        <Image
          src={cat}
          alt="kazusa cat"
          className="size-36 bg-white rounded-full"
        />
        <div>
          <p className={`text-6xl tracking-wider ${zenKurenaido.className}`}>
            放課後スイーツ部
          </p>
          <p className={`text-lg tracking-wider ${zenKurenaido.className}`}>
            After-School Sweets Club
          </p>
        </div>
      </div>
      <div className="flex-1 p-4 md:pl-0">
        <ThemeSwitcher />
      </div>
      {children}
    </main>
  );
}
