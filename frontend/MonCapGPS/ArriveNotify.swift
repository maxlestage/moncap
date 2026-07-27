import ContactsUI
import MessageUI
import SwiftUI

/// Présente le sélecteur de contacts natif (CNContactPickerViewController)
/// **depuis un contrôleur hôte stable et invisible**, et non via une `.sheet`
/// SwiftUI imbriquée : cette dernière faisait tomber toute la pile de feuilles
/// à la sélection (« tout quitte »). Aucune autorisation Contacts requise.
struct ContactPickerPresenter: UIViewControllerRepresentable {
    @Binding var isPresented: Bool
    var onPick: (String, String) -> Void

    func makeUIViewController(context: Context) -> UIViewController {
        UIViewController()
    }

    func updateUIViewController(_ host: UIViewController, context: Context) {
        guard isPresented, host.presentedViewController == nil else { return }
        let picker = CNContactPickerViewController()
        picker.delegate = context.coordinator
        picker.predicateForEnablingContact = NSPredicate(format: "phoneNumbers.@count > 0")
        host.present(picker, animated: true)
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, CNContactPickerDelegate {
        let parent: ContactPickerPresenter
        init(_ parent: ContactPickerPresenter) { self.parent = parent }

        func contactPicker(_ picker: CNContactPickerViewController, didSelect contact: CNContact) {
            let name = [contact.givenName, contact.familyName]
                .filter { !$0.isEmpty }
                .joined(separator: " ")
            let phone = contact.phoneNumbers.first?.value.stringValue ?? ""
            parent.onPick(name.isEmpty ? phone : name, phone)
            parent.isPresented = false
        }

        func contactPickerDidCancel(_ picker: CNContactPickerViewController) {
            parent.isPresented = false
        }
    }
}

/// Présente le compositeur SMS natif (MFMessageComposeViewController) depuis un
/// contrôleur hôte stable (même raison que ci-dessus). Pré-rempli destinataire
/// + message ; l'utilisateur relit et envoie lui-même.
struct MessageComposerPresenter: UIViewControllerRepresentable {
    @Binding var isPresented: Bool
    let recipients: [String]
    let body: String
    var onFinish: () -> Void

    func makeUIViewController(context: Context) -> UIViewController {
        UIViewController()
    }

    func updateUIViewController(_ host: UIViewController, context: Context) {
        guard isPresented, host.presentedViewController == nil,
            MFMessageComposeViewController.canSendText()
        else { return }
        let vc = MFMessageComposeViewController()
        vc.recipients = recipients
        vc.body = body
        vc.messageComposeDelegate = context.coordinator
        host.present(vc, animated: true)
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, MFMessageComposeViewControllerDelegate {
        let parent: MessageComposerPresenter
        init(_ parent: MessageComposerPresenter) { self.parent = parent }

        func messageComposeViewController(
            _ controller: MFMessageComposeViewController, didFinishWith result: MessageComposeResult
        ) {
            // MFMessageComposeViewController ne se referme pas tout seul.
            controller.dismiss(animated: true) { [parent] in
                parent.isPresented = false
                parent.onFinish()
            }
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
            // Sélecteur de contacts et compositeur SMS présentés depuis un hôte
            // stable (invisible), pour ne pas casser la pile de feuilles.
            .background(
                ContactPickerPresenter(isPresented: $showContactPicker) { name, phone in
                    contactName = name
                    contactPhone = phone
                }
            )
            .background(
                MessageComposerPresenter(
                    isPresented: $showComposer,
                    recipients: [contactPhone],
                    body: message
                ) {
                    dismiss()
                }
            )
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
