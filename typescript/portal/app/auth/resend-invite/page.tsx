import { Card, Link, Separator } from "@heroui/react";
import { ResendForm } from "./resend-form";

export default function ResendInvitePage() {
  return (
    <Card>
      <Card.Header>
        <Card.Title>Resend Invite</Card.Title>
        <Card.Description>
          When you should receive an invite email but the email is not
          delivered, use this form to resend it.
        </Card.Description>
        <Card.Content className="flex flex-col md:flex-row">
          <ResendForm />
          <Separator orientation="vertical" className="hidden md:block" />
          <Separator orientation="horizontal" className="block md:hidden" />
          <div className="flex-1 p-4">
            <p>
              Accidentally jumped to this page?
            </p>
            <p>
              <Link href="/auth/login" className="text-accent">
                Return to login
                <Link.Icon />
              </Link>
            </p>
          </div>
        </Card.Content>
      </Card.Header>
    </Card>
  );
}
