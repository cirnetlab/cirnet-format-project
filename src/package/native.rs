use super::{
    Compression, Inventory, Item, MANIFEST_ENTRY, Operation, PackagePhase, PackageProgress,
    PackagePublishError, PackagePublishOutcome, PackageSource, path,
    publication::{Destination, install_file, sync_publication_directory},
    read::{Inspected, Stage},
    read_bounded, write,
};
use crate::{
    Digest, Error, Limits, Manifest, ProjectPath, Result,
    error::io,
    integrity::transfer_with,
    storage::{
        FilePublicationOutcome, Project, open_regular, persist_new, resolve, temp_bytes,
        trusted_root, write_lock,
    },
};
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Seek, Write},
    ops::ControlFlow,
    path::Path,
};
use tempfile::{NamedTempFile, TempDir};

struct NativeSource<'a>(&'a Path);

impl PackageSource for NativeSource<'_> {
    type Reader<'a>
        = File
    where
        Self: 'a;

    fn open(&self, path: &ProjectPath) -> Result<File> {
        open_regular(self.0, path)
    }
}

/// Write a deterministic ZIP to an empty seekable sink under the project's writer lock.
/// On failure discard the sink.
pub fn export<W: Write + Seek>(sink: W, project: &Project, compression: Compression) -> Result<W> {
    export_with(
        sink,
        project,
        compression,
        &mut Operation::quiet(PackagePhase::Export),
    )
}

/// The callback runs under the writer lock during export. It must not start a
/// second write or export on the same project.
pub fn export_with_progress<W: Write + Seek>(
    sink: W,
    project: &Project,
    compression: Compression,
    mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
) -> Result<W> {
    export_with(
        sink,
        project,
        compression,
        &mut Operation::new(PackagePhase::Export, &mut progress),
    )
}

fn export_with<W: Write + Seek>(
    mut sink: W,
    project: &Project,
    compression: Compression,
    operation: &mut Operation<'_>,
) -> Result<W> {
    write::check_empty_sink(&mut sink)?;
    operation.check(None, 0)?;
    let _lock = write_lock(project.root())?;
    project.ensure_no_pending()?;
    project.check_revision()?;
    let sink = write::export_into_empty(
        sink,
        project.manifest(),
        &NativeSource(project.root()),
        compression,
        project.limits(),
        operation,
    )?;
    project.check_revision()?;
    Ok(sink)
}

/// Atomically publish a new package without replacing an existing file.
pub fn export_file(
    path: impl AsRef<Path>,
    project: &Project,
    compression: Compression,
) -> Result<()> {
    export_file_with_outcome(path, project, compression)?.into_result()
}

/// Export without replacing an existing file, separating a published package
/// from any post-installation durability warning.
pub fn export_file_with_outcome(
    path: impl AsRef<Path>,
    project: &Project,
    compression: Compression,
) -> Result<FilePublicationOutcome> {
    export_file_with(
        path.as_ref(),
        project,
        compression,
        &mut Operation::quiet(PackagePhase::Export),
    )
}

/// Export to a new file with cooperative cancellation. The callback must not
/// start another write or export on the same project.
pub fn export_file_with_progress(
    path: impl AsRef<Path>,
    project: &Project,
    compression: Compression,
    progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
) -> Result<()> {
    export_file_with_progress_and_outcome(path, project, compression, progress)?.into_result()
}

/// Export cooperatively. Cancellation rejects before installation. A warning in
/// the successful outcome means the package is already installed.
pub fn export_file_with_progress_and_outcome(
    path: impl AsRef<Path>,
    project: &Project,
    compression: Compression,
    mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
) -> Result<FilePublicationOutcome> {
    export_file_with(
        path.as_ref(),
        project,
        compression,
        &mut Operation::new(PackagePhase::Export, &mut progress),
    )
}

fn export_file_with(
    path: &Path,
    project: &Project,
    compression: Compression,
    operation: &mut Operation<'_>,
) -> Result<FilePublicationOutcome> {
    let (parent, name) = destination_parts(path)?;
    if name.extension() != Some("cirpkg") {
        return Err(Error::Path(name.to_string()));
    }
    let parent = trusted_root(parent)?;
    let mut temp = NamedTempFile::new_in(&parent).map_err(|error| io(name.as_str(), error))?;
    export_with(temp.as_file_mut(), project, compression, operation)?;
    temp.as_file()
        .sync_all()
        .map_err(|error| io(name.as_str(), error))?;
    operation.check(None, 0)?;
    persist_new(temp, &parent.join(name.as_path()))
}

pub(super) fn destination_parts(destination: &Path) -> Result<(&Path, ProjectPath)> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Path(destination.display().to_string()))?;
    let name = ProjectPath::new(name)?;
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok((parent, name))
}

struct DirectoryStage(TempDir);

impl Stage for DirectoryStage {
    type Writer = File;

    fn create(&mut self, name: &ProjectPath) -> Result<File> {
        let destination = resolve(self.0.path(), name, true)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(destination)
            .map_err(|error| io(name.as_str(), error))
    }

    fn finish(&mut self, name: &ProjectPath, writer: File) -> Result<()> {
        writer.sync_all().map_err(|error| io(name.as_str(), error))
    }
}

pub struct VerifiedPackage {
    directory: Option<TempDir>,
    manifest: Manifest,
    inventory: Inventory,
    limits: Limits,
}

/// Validate every member into private scratch storage. No import destination is opened here.
/// The scratch parent must be a local directory controlled by the caller.
pub fn verify<R: Read + Seek>(
    reader: R,
    scratch_parent: impl AsRef<Path>,
    limits: Limits,
) -> Result<VerifiedPackage> {
    verify_with(
        reader,
        scratch_parent.as_ref(),
        limits,
        &mut Operation::quiet(PackagePhase::Verify),
    )
}

/// Verify into scratch storage with cooperative cancellation. Scratch is removed on failure.
pub fn verify_with_progress<R: Read + Seek>(
    reader: R,
    scratch_parent: impl AsRef<Path>,
    limits: Limits,
    mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
) -> Result<VerifiedPackage> {
    verify_with(
        reader,
        scratch_parent.as_ref(),
        limits,
        &mut Operation::new(PackagePhase::Verify, &mut progress),
    )
}

fn verify_with<R: Read + Seek>(
    reader: R,
    scratch_parent: &Path,
    limits: Limits,
    operation: &mut Operation<'_>,
) -> Result<VerifiedPackage> {
    let inspected = Inspected::open(reader, &limits, operation)?;
    let mut builder = tempfile::Builder::new();
    builder.prefix(".cirnet-verify-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let mut stage = DirectoryStage(
        builder
            .tempdir_in(trusted_root(scratch_parent)?)
            .map_err(|error| io("private package stage", error))?,
    );
    let (manifest, inventory) = inspected.verify(&limits, &mut stage, operation)?;
    Ok(VerifiedPackage {
        directory: Some(stage.0),
        manifest,
        inventory,
        limits,
    })
}

impl VerifiedPackage {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Publish to a new directory. The manifest is installed after all its resources.
    /// A destination on another volume is supported by bounded copies.
    pub fn publish(mut self, destination: impl AsRef<Path>) -> Result<Project> {
        let outcome = self.publish_with(
            destination.as_ref(),
            &mut Operation::quiet(PackagePhase::Publish),
        )?;
        legacy_result(outcome)
    }

    /// Publish with cooperative cancellation. Use [Self::publish_retryable] to keep
    /// the staged package after a clean rejection or inspect post-publication warnings.
    pub fn publish_with_progress(
        mut self,
        destination: impl AsRef<Path>,
        progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
    ) -> Result<Project> {
        legacy_result(self.publish_retryable(destination, progress)?)
    }

    /// Publish under a trusted local parent. The callback must not alter the import paths.
    /// Clean rejections allow a retry using this verified stage. Publication and incomplete
    /// cleanup consume the stage because linked files may become visible to other writers.
    pub fn publish_retryable(
        &mut self,
        destination: impl AsRef<Path>,
        mut progress: impl FnMut(PackageProgress<'_>) -> ControlFlow<()>,
    ) -> std::result::Result<PackagePublishOutcome, PackagePublishError> {
        self.publish_with(
            destination.as_ref(),
            &mut Operation::new(PackagePhase::Publish, &mut progress),
        )
    }

    fn publish_with(
        &mut self,
        destination: &Path,
        operation: &mut Operation<'_>,
    ) -> std::result::Result<PackagePublishOutcome, PackagePublishError> {
        operation.check(None, 0)?;
        let verified = self.directory.as_ref().ok_or_else(|| {
            Error::Validation("the verified package stage has already been consumed".into())
        })?;
        let (parent, name) = destination_parts(destination)?;
        let parent = trusted_root(parent)?;
        let mut destination = Destination::create(parent.join(name.as_path()))?;
        let manifest_name = path(MANIFEST_ENTRY);
        let result = publish_verified(
            verified.path(),
            &self.inventory,
            &mut destination,
            &manifest_name,
            operation,
        );
        match result {
            Ok((revision, warning)) => {
                self.directory.take();
                Ok(PackagePublishOutcome {
                    project: Project::from_parts(
                        destination.root,
                        manifest_name,
                        self.manifest.clone(),
                        revision,
                        self.limits,
                    ),
                    warning,
                })
            }
            Err(cause) => match destination.rollback() {
                Ok(()) => Err(cause.into()),
                Err(cleanup) => {
                    self.directory.take();
                    Err(PackagePublishError::Incomplete {
                        destination: destination.root,
                        cause: Box::new(cause),
                        cleanup: Box::new(cleanup),
                    })
                }
            },
        }
    }
}

fn legacy_result(outcome: PackagePublishOutcome) -> Result<Project> {
    match outcome.warning {
        None => Ok(outcome.project),
        Some(error) => Err(error),
    }
}

fn publish_verified<'a>(
    verified: &Path,
    inventory: &'a Inventory,
    destination: &mut Destination<'a>,
    manifest_name: &ProjectPath,
    operation: &mut Operation<'_>,
) -> Result<(Digest, Option<Error>)> {
    destination.start()?;
    for (name, expected) in &inventory.entries {
        if name != manifest_name {
            publish_resource(verified, destination, name, expected, operation)?;
        }
    }
    operation.check(Some(MANIFEST_ENTRY), 0)?;
    let expected = &inventory.entries[manifest_name];
    let bytes = read_bounded(
        open_regular(verified, manifest_name)?,
        expected.bytes,
        MANIFEST_ENTRY,
        |bytes| operation.check(Some(MANIFEST_ENTRY), bytes),
    )?;
    let revision = Digest::of(&bytes);
    if bytes.len() as u64 != expected.bytes || revision != expected.sha256 {
        return Err(Error::Integrity(MANIFEST_ENTRY.into()));
    }
    let file = temp_bytes(&destination.root, &bytes)?;
    let target = destination.root.join(manifest_name.as_path());
    operation.check(None, 0)?;
    destination.ensure_path(&target)?;
    install_file(file, &target)?;

    // The manifest installation is the commit point. Keep it even if cleanup or sync fails.
    let warning = sync_publication_directory(&destination.root, &target)
        .and_then(|()| {
            destination
                .remove_marker()
                .map_err(|error| Error::PublishedCleanup(Box::new(error)))
        })
        .err();
    Ok((revision, warning))
}

fn publish_resource<'a>(
    verified: &Path,
    destination: &mut Destination<'a>,
    name: &ProjectPath,
    expected: &'a Item,
    operation: &mut Operation<'_>,
) -> Result<()> {
    let context = name.as_str();
    operation.check(Some(context), 0)?;
    let target = destination.resource_path(name)?;
    let parent = target.parent().expect("target has parent");
    let staged = open_regular(verified, name)?;
    let metadata = staged.metadata().map_err(|error| io(context, error))?;
    if metadata.len() != expected.bytes {
        return Err(Error::Integrity(name.to_string()));
    }

    // Windows files can preserve source ACLs when linked. Keep that fast path
    // within the same trusted parent. Other destinations get locally created files.
    let link = cfg!(unix) || verified.parent() == destination.root.parent();
    if link && link_staged(&verified.join(name.as_path()), &target)? {
        destination.record(target, metadata, expected)?;
        return operation.check(Some(context), expected.bytes);
    }

    let mut temp = NamedTempFile::new_in(parent).map_err(|error| io(context, error))?;
    let (count, sha256) = transfer_with(
        staged,
        temp.as_file_mut(),
        expected.bytes,
        context,
        |bytes| operation.check(Some(context), bytes),
    )?;
    if count != expected.bytes || sha256 != expected.sha256 {
        return Err(Error::Integrity(name.to_string()));
    }
    temp.as_file()
        .sync_all()
        .map_err(|error| io(context, error))?;
    destination.ensure_path(&target)?;
    let metadata = install_file(temp, &target)?;
    destination.record(target, metadata, expected)
}

// Fast path after staging has verified and synced the source. The caller records
// the link before attempting destination synchronization. Other filesystems fall back to copies.
fn link_staged(source: &Path, target: &Path) -> Result<bool> {
    match fs::hard_link(source, target) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            Err(Error::Conflict(target.display().to_string()))
        }
        Err(_) => Ok(false),
    }
}
