/// The words for what the document check found. The Rust core reports typed
/// values only, so all user-facing wording lives here, in one place.
library;

import '../src/rust/api/document.dart';

/// Shown when a damaged file opened after repair.
const repairedTitle = 'This file was damaged';
const repairedBody =
    'PaperWarden repaired it so it could open. Some pages or content may be '
    'missing or look different. Your original file was not changed.';

/// Shown when content that could act on its own was found and not run.
const notRunTitle = 'Some content was not run';
const notRunIntro = 'PaperWarden never runs these or opens links by itself:';

/// Shown when a document is refused.
const refusedTitle = "PaperWarden can't open this file";
const refusedFooter =
    'Nothing in the file was run, and your original file was not changed.';

/// One line for each kind of content that was not run.
String notRunItem(DocumentNotice notice) => switch (notice) {
      DocumentNotice.scripts => 'Scripts',
      DocumentNotice.launchActions =>
        'Actions that would open other apps or files',
      DocumentNotice.webLinks => 'Links to websites',
      DocumentNotice.formSubmission =>
        'A form that would send its answers online',
      DocumentNotice.dataImport =>
        'An action that would import data from another file',
      DocumentNotice.otherDocuments => 'Links that would open other documents',
      DocumentNotice.embeddedFiles => 'Files attached inside the document',
      DocumentNotice.media => 'Video, sound or 3D content',
      DocumentNotice.interactiveForms =>
        'An interactive form that needs scripts',
      // Not content: the repair has its own notice.
      DocumentNotice.repaired => '',
    };

/// Why a document was refused, in plain words.
String refusalReason(RefusalReason reason) => switch (reason) {
      RefusalReason.notPdf => "This file isn't a PDF, whatever its name says.",
      RefusalReason.noContent => 'This PDF is empty or too damaged to repair.',
      RefusalReason.encrypted =>
        "This PDF is encrypted. Opening encrypted PDFs isn't supported yet.",
      RefusalReason.tooLarge =>
        'This file is larger than PaperWarden can open safely.',
      RefusalReason.nestedTooDeeply =>
        'This PDF is built in a way real documents never are, which is a '
            'common sign of an attack.',
      RefusalReason.tooManyParts =>
        'This PDF has more parts than PaperWarden can open safely.',
      RefusalReason.tooManyRevisions =>
        'This PDF has been saved over more times than PaperWarden can open '
            'safely.',
      RefusalReason.expandsTooMuch =>
        'Part of this PDF would expand to far more data than real documents '
            'hold, which is a common sign of an attack.',
      RefusalReason.imageTooLarge =>
        'This PDF contains an image too large to open safely.',
    };
