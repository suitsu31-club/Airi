import { Card } from "@heroui/react";
import RegisterForm from "./register-form";

export default function RegisterPage() {
  return (
    <Card>
      <Card.Header>
        <Card.Title>Register</Card.Title>
      </Card.Header>
      <Card.Content>
        <RegisterForm />
      </Card.Content>
    </Card>
  );
}
