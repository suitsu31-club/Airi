import { Card, Link } from "@heroui/react";
import { LoginForm } from "./login-form";

export default function LoginPage() {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <Card className="row-span-1">
        <Card.Header>
          <Card.Title>Login</Card.Title>
        </Card.Header>
        <Card.Content>
          <LoginForm />
        </Card.Content>
        <Card.Footer></Card.Footer>
      </Card>
      <Card className="row-span-2">
        <Card.Header>
          <Card.Title>How to register</Card.Title>
        </Card.Header>
        <Card.Content>
          <p>We are a private community that doesn't open registration.</p>
          <p>
            To register, you must reach any of our members and request an
            invite. If he is willing to give you an invite, you can use the link
            in the invite email to register.
          </p>
          <p>
            If you did get invited, but didn't receive the invite email, you can
            request a resend from &nbsp;
            <Link href="/auth/resend-invite" className="text-accent">
              resend invite form
              <Link.Icon />
            </Link>
          </p>
        </Card.Content>
        <Card.Footer></Card.Footer>
      </Card>
    </div>
  );
}
