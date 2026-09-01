import {
  Button,
  FieldError,
  Form,
  Input,
  Label,
  TextField,
} from "@heroui/react";

export function ResendForm() {
  return (
    <Form className="flex-1 py-4 md:pb-16 px-4 md:pl-0 space-y-4">
      <TextField isRequired name="email" type="email">
        <Label>Email</Label>
        <Input placeholder="Enter your email" />
        <FieldError />
      </TextField>
      <Button type="submit">Resend</Button>
    </Form>
  );
}
