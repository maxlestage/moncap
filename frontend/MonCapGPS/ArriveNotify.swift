import ContactsUI
import MessageUI
import SwiftUI

/// Sélecteur de contacts natif iOS (CNContactPickerViewController). Il s'exécute
/// hors-processus : **aucune autorisation Contacts n'est requise**, on ne reçoit
/// que le contact choisi.
struct ContactPicker: UIViewControllerRepresentable {
    /// (nom affiché, numéro de téléphone).
    var onPick: (String, String) -> Void
    var onCancel: () -> Void = {}

    func makeUIViewController(context: Context) -> CNContactPickerViewController {
        let picker = CNContactPickerViewController()
        picker.delegate = context.coordinator
        // Ne rend sélectionnables que les contacts ayant au moins un numéro.
        picker.predicateForEnablingContact = NSPredicate(format: "phoneNumbers.@count > 0")
        return picker
    }

    func updateUIViewController(_ vc: CNContactPickerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, CNContactPickerDelegate {
        let parent: ContactPicker
        init(_ parent: ContactPicker) { self.parent = parent }

        func contactPicker(_ picker: CNContactPickerViewController, didSelect contact: CNContact) {
            let name = [contact.givenName, contact.familyName]
                .filter { !$0.isEmpty }
                .joined(separator: " ")
            let phone = contact.phoneNumbers.first?.value.stringValue ?? ""
            parent.onPick(name.isEmpty ? phone : name, phone)
        }

        func contactPickerDidCancel(_ picker: CNContactPickerViewController) {
            parent.onCancel()
        }
    }
}

/// Compositeur de SMS natif (MFMessageComposeViewController), pré-rempli avec le
/// destinataire et le message. L'utilisateur relit et envoie lui-même.
struct MessageComposer: UIViewControllerRepresentable {
    let recipients: [String]
    let body: String
    var onFinish: () -> Void

    func makeUIViewController(context: Context) -> MFMessageComposeViewController {
        let vc = MFMessageComposeViewController()
        vc.recipients = recipients
        vc.body = body
        vc.messageComposeDelegate = context.coordinator
        return vc
    }

    func updateUIViewController(_ vc: MFMessageComposeViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(onFinish: onFinish) }

    final class Coordinator: NSObject, MFMessageComposeViewControllerDelegate {
        let onFinish: () -> Void
        init(onFinish: @escaping () -> Void) { self.onFinish = onFinish }

        func messageComposeViewController(
            _ controller: MFMessageComposeViewController, didFinishWith result: MessageComposeResult
        ) {
            onFinish()
        }
    }
}

/// « Prévenir un proche » : choisir un contact, un message pré-rempli et
/// modifiable (ou une suggestion), puis envoyer par SMS.
struct ArriveNotifyView: View {
    @Environment(\.dismiss) private var dismiss

    @State private var message: String
    @State private var contactName = ""
    @State private var contactPhone = ""
    @State private var showContactPicker = false
    @State private var showComposer = false
    @State private var cannotSendText = false

    /// Suggestions de messages d'arrivée.
    static let presets = [
        "Je suis bien arrivé·e 🙌",
        "Arrivé·e à destination, tout va bien ✅",
        "Je viens d'arriver 📍",
        "Bien rentré·e à la maison 🏡",
    ]

    init() { _message = State(initialValue: ArriveNotifyView.presets[0]) }

    private var canSend: Bool {
        !contactPhone.isEmpty && !message.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Message") {
                    TextField("Ton message", text: $message, axis: .vertical)
                        .lineLimit(2...4)
                }
                Section("Suggestions") {
                    ForEach(Self.presets, id: \.self) { preset in
                        Button {
                            message = preset
                        } label: {
                            HStack {
                                Text(preset).foregroundStyle(.primary)
                                Spacer()
                                if message == preset {
                                    Image(systemName: "checkmark").foregroundStyle(.blue)
                                }
                            }
                        }
                    }
                }
                Section("Destinataire") {
                    Button {
                        showContactPicker = true
                    } label: {
                        Label(
                            contactName.isEmpty ? "Choisir un contact" : contactName,
                            systemImage: "person.crop.circle")
                    }
                    if !contactPhone.isEmpty {
                        Text(contactPhone)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
                Section {
                    Button {
                        if MFMessageComposeViewController.canSendText() {
                            showComposer = true
                        } else {
                            cannotSendText = true
                        }
                    } label: {
                        Label("Envoyer le message", systemImage: "paperplane.fill")
                            .frame(maxWidth: .infinity)
                            .fontWeight(.semibold)
                    }
                    .disabled(!canSend)
                }
            }
            .navigationTitle("Prévenir un proche")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Fermer") { dismiss() }
                }
            }
            .sheet(isPresented: $showContactPicker) {
                ContactPicker(
                    onPick: { name, phone in
                        contactName = name
                        contactPhone = phone
                        showContactPicker = false
                    },
                    onCancel: { showContactPicker = false })
            }
            .sheet(isPresented: $showComposer) {
                MessageComposer(recipients: [contactPhone], body: message) {
                    showComposer = false
                    dismiss()
                }
            }
            .alert("Messages indisponible", isPresented: $cannotSendText) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(
                    "Cet appareil ne peut pas envoyer de SMS. Copie ton message et envoie-le autrement."
                )
            }
        }
    }
}
