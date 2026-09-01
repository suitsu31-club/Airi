export default function AuthLayout({ children }: { children: React.ReactNode }) {
  return (
    <main className="container mx-auto mt-64">
      {children}
    </main>
  );
}
